//! Clone engine, reverse-engineered from Rescuezilla's clone_manager.py +
//! restore_manager.py (is_cloning branch).
//!
//! Flow:
//!  1. If overwrite_partition_table: `sfdisk --dump SRC | sfdisk DEST`
//!  2. For each enabled partition mapping: `partclone.<fs> --dev-to-dev
//!     --source SRC --overwrite DEST` (or `partclone.dd` for raw), parsing the
//!     stderr progress stream and emitting byte-weighted overall progress.
//!  3. `sync`.
//!
//! partclone is Linux-only and requires root. This module only compiles its
//! real implementation on Linux; elsewhere start_clone returns Unsupported.

use crate::model::{
    CloneError, CloneProgressEvent, CloneRequest, CloneSummary,
};
#[cfg(target_os = "linux")]
use crate::model::PartitionProgress;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A cancellation flag shared with the running clone.
#[derive(Default, Clone)]
pub struct CancelToken(pub Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// partclone's status line parser. partclone prints lines like:
/// "Elapsed: 00:00:12, Remaining: 00:00:48, Completed: 23.45%".
/// Returns the completed fraction 0..1 when found.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_partclone_fraction(line: &str) -> Option<f64> {
    let marker = "Completed:";
    let idx = line.find(marker)?;
    let rest = &line[idx + marker.len()..];
    let pct: String = rest
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    pct.parse::<f64>().ok().map(|p| (p / 100.0).clamp(0.0, 1.0))
}

#[cfg(target_os = "linux")]
pub fn run_clone<F>(
    request: &CloneRequest,
    cancel: &CancelToken,
    mut emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    use crate::drives::list_drives;
    use crate::logger::OperationLog;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::time::Instant;

    crate::preflight::require_ready()?;
    let log = OperationLog::open("clone");
    let started = Instant::now();
    let drives = list_drives()?;

    // Destination safety guardrail (Milestone 5.3).
    crate::safety::enforce_destination(
        &drives,
        &request.destination_drive_key,
        Some(&request.source_drive_key),
        request.confirm_device.as_deref(),
    )?;
    log.line(&format!(
        "clone {} -> {} (overwrite_table={}, rescue={})",
        request.source_drive_key,
        request.destination_drive_key,
        request.overwrite_partition_table,
        request.rescue
    ));

    let source = drives
        .iter()
        .find(|d| d.key == request.source_drive_key)
        .ok_or_else(|| CloneError::Other("source drive not found".into()))?;

    let active: Vec<_> = request
        .mapping
        .iter()
        .filter(|m| m.enabled && m.dest_key != "DISABLED")
        .collect();

    // Byte weighting: used bytes where known, else partition size, else 1.
    let weight_of = |src_key: &str| -> u64 {
        source
            .partitions
            .iter()
            .find(|p| p.key == src_key)
            .map(|p| p.used_bytes.unwrap_or(p.size_bytes).max(1))
            .unwrap_or(1)
    };
    let total_weight: u64 = active.iter().map(|m| weight_of(&m.source_key)).sum::<u64>().max(1);

    // Step 1: optionally copy the partition table.
    if request.overwrite_partition_table {
        let cmd = format!(
            "sfdisk --dump {} | sfdisk {}",
            request.source_drive_key, request.destination_drive_key
        );
        log.command(&cmd);
        emit(CloneProgressEvent {
            status: "preparing".into(),
            overall_fraction: 0.0,
            current: None,
            message: "Writing destination partition table…".into(),
            log_line: Some(cmd),
        });
        let dump = Command::new("sfdisk")
            .args(["--dump", &request.source_drive_key])
            .output()
            .map_err(|e| CloneError::Command("sfdisk --dump".into(), e.to_string()))?;
        if !dump.status.success() {
            return Err(CloneError::Command(
                "sfdisk --dump".into(),
                String::from_utf8_lossy(&dump.stderr).to_string(),
            ));
        }
        let mut writer = Command::new("sfdisk")
            .arg(&request.destination_drive_key)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CloneError::Command("sfdisk write".into(), e.to_string()))?;
        use std::io::Write;
        writer
            .stdin
            .take()
            .unwrap()
            .write_all(&dump.stdout)
            .map_err(|e| CloneError::Command("sfdisk write".into(), e.to_string()))?;
        let status = writer
            .wait()
            .map_err(|e| CloneError::Command("sfdisk write".into(), e.to_string()))?;
        if !status.success() {
            return Err(CloneError::Command(
                "sfdisk write".into(),
                "failed to write destination partition table".into(),
            ));
        }
    }

    // Step 2: copy each partition.
    let mut completed_weight: u64 = 0;
    let mut lines = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let log_path = Some(log.path().to_string_lossy().to_string());

    for m in &active {
        if cancel.is_cancelled() {
            let s = summary("cancelled", started.elapsed().as_secs(), lines, warnings, log_path.clone());
            log.summary(&s.status, &s.lines, s.duration_seconds);
            return Ok(s);
        }
        let weight = weight_of(&m.source_key);
        let fs = source
            .partitions
            .iter()
            .find(|p| p.key == m.source_key)
            .and_then(|p| p.filesystem.clone())
            .unwrap_or_default();
        let tool = partclone_tool(&fs);

        let cmd = format!(
            "{tool} --dev-to-dev --source {} --overwrite {}{}",
            m.source_key,
            m.dest_key,
            if request.rescue { " --rescue" } else { "" }
        );
        log.command(&cmd);
        emit(CloneProgressEvent {
            status: "copying".into(),
            overall_fraction: completed_weight as f64 / total_weight as f64,
            current: Some(PartitionProgress {
                source_key: m.source_key.clone(),
                dest_key: m.dest_key.clone(),
                fraction: 0.0,
                status_line: "Starting…".into(),
            }),
            message: format!("Cloning {} → {}", m.source_key, m.dest_key),
            log_line: Some(cmd),
        });

        let mut args: Vec<String> = vec![
            "--dev-to-dev".into(),
            "--source".into(),
            m.source_key.clone(),
            "--overwrite".into(),
            m.dest_key.clone(),
        ];
        if request.rescue {
            args.push("--rescue".into());
        }

        let mut child = Command::new(&tool)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CloneError::Command(tool.clone(), e.to_string()))?;

        let stderr = child.stderr.take().unwrap();
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if cancel.is_cancelled() {
                let _ = child.kill();
                let s = summary("cancelled", started.elapsed().as_secs(), lines, warnings, log_path.clone());
                log.summary(&s.status, &s.lines, s.duration_seconds);
                return Ok(s);
            }
            if let Some(frac) = parse_partclone_fraction(&line) {
                let overall = (completed_weight as f64 + weight as f64 * frac) / total_weight as f64;
                emit(CloneProgressEvent {
                    status: "copying".into(),
                    overall_fraction: overall,
                    current: Some(PartitionProgress {
                        source_key: m.source_key.clone(),
                        dest_key: m.dest_key.clone(),
                        fraction: frac,
                        status_line: line.trim().to_string(),
                    }),
                    message: format!("Cloning {} → {}", m.source_key, m.dest_key),
                    log_line: None,
                });
            }
        }

        let status = child
            .wait()
            .map_err(|e| CloneError::Command(tool.clone(), e.to_string()))?;
        if !status.success() {
            if request.rescue {
                // Rescue mode: a non-zero exit is treated as non-fatal (bad
                // sectors skipped). Record a warning and continue. (5.7)
                let warn = format!(
                    "{} → {}: completed with errors (rescue mode continued past bad sectors)",
                    m.source_key, m.dest_key
                );
                log.line(&format!("WARNING: {warn}"));
                warnings.push(warn);
                lines.push(format!("{} → {}: cloned with recoverable errors", m.source_key, m.dest_key));
            } else {
                lines.push(format!("{} → {}: FAILED", m.source_key, m.dest_key));
                let s = summary("failed", started.elapsed().as_secs(), lines, warnings, log_path.clone());
                log.summary(&s.status, &s.lines, s.duration_seconds);
                return Ok(s);
            }
        } else {
            lines.push(format!("{} → {}: cloned successfully", m.source_key, m.dest_key));
        }

        // Optional post-op verification (5.4).
        if request.verify && !cancel.is_cancelled() {
            let bytes = weight; // used bytes for this partition
            emit(CloneProgressEvent {
                status: "finalizing".into(),
                overall_fraction: (completed_weight + weight) as f64 / total_weight as f64,
                current: None,
                message: format!("Verifying {} …", m.dest_key),
                log_line: Some(format!("verify: sha256({}) vs sha256({})", m.source_key, m.dest_key)),
            });
            match crate::verify::compare_devices(&m.source_key, &m.dest_key, bytes, cancel) {
                Ok(true) => log.line(&format!("verify {} OK", m.dest_key)),
                Ok(false) => {
                    let w = format!("verification FAILED for {} (source and destination differ)", m.dest_key);
                    log.line(&format!("WARNING: {w}"));
                    warnings.push(w);
                }
                Err(e) => {
                    let w = format!("verification of {} could not run: {e}", m.dest_key);
                    log.line(&format!("WARNING: {w}"));
                    warnings.push(w);
                }
            }
        }
        completed_weight += weight;
    }

    // Step 3: flush.
    emit(CloneProgressEvent {
        status: "finalizing".into(),
        overall_fraction: 1.0,
        current: None,
        message: "Flushing buffers…".into(),
        log_line: Some("sync".into()),
    });
    let _ = Command::new("sync").status();

    let s = summary("completed", started.elapsed().as_secs(), lines, warnings, log_path);
    log.summary(&s.status, &s.lines, s.duration_seconds);
    Ok(s)
}

#[cfg(target_os = "windows")]
pub fn run_clone<F>(
    request: &CloneRequest,
    cancel: &CancelToken,
    mut emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    crate::windows::run_clone_windows(request, cancel, &mut emit)
}

#[cfg(target_os = "macos")]
pub fn run_clone<F>(
    request: &CloneRequest,
    cancel: &CancelToken,
    mut emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    crate::macos::run_clone_macos(request, cancel, &mut emit)
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn run_clone<F>(
    _request: &CloneRequest,
    _cancel: &CancelToken,
    _emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    Err(CloneError::UnsupportedPlatform)
}

/// Map a filesystem to the specialised partclone binary, as Rescuezilla does.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn partclone_tool(fs: &str) -> String {
    let name = match fs {
        "ntfs" => "ntfs",
        "ext2" | "ext3" | "ext4" => "extfs",
        "fat" | "fat12" | "fat16" | "fat32" | "vfat" => "fat",
        "exfat" => "exfat",
        "xfs" => "xfs",
        "btrfs" => "btrfs",
        "hfsplus" => "hfsp",
        "" => "dd",
        _ => "dd",
    };
    format!("partclone.{name}")
}

#[cfg(target_os = "linux")]
fn summary(
    status: &str,
    duration: u64,
    lines: Vec<String>,
    warnings: Vec<String>,
    log_path: Option<String>,
) -> CloneSummary {
    let title = match status {
        "completed" => "Clone completed successfully",
        "cancelled" => "Clone cancelled",
        _ => "Clone failed",
    }
    .to_string();
    let mut lines = lines;
    if status == "cancelled" {
        lines.push("Operation cancelled by user.".into());
    }
    CloneSummary {
        status: status.to_string(),
        title,
        lines,
        duration_seconds: duration,
        warnings,
        log_path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partclone_progress_line() {
        let line = "Elapsed: 00:01:38, Remaining: 00:00:28, Completed:  77.58%, 2.57MB/min";
        let f = parse_partclone_fraction(line).unwrap();
        assert!((f - 0.7758).abs() < 1e-6, "got {f}");
    }

    #[test]
    fn partclone_progress_none() {
        assert!(parse_partclone_fraction("Partclone v0.3.x starting").is_none());
    }

    #[test]
    fn partclone_progress_bounds() {
        assert_eq!(parse_partclone_fraction("Completed: 100.00%"), Some(1.0));
        assert_eq!(parse_partclone_fraction("Completed: 0.00%"), Some(0.0));
    }
}
