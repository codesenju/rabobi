//! Backup engine, reverse-engineered from Rescuezilla's backup_manager.py.
//!
//! Writes a Clonezilla-compatible image directory and, per partition, runs:
//!   partclone.<fs> --clone --source <part> --output - | <compressor> | split
//! producing <part>.<fs>-ptcl-img.<suffix>.aa, .ab, …
//!
//! Also writes the metadata files restore needs: parts, disk, <disk>-pt.sf,
//! <disk>-mbr, dev-fs.list, <part>.info (swap), rescuezilla.description.txt.
//!
//! Linux + root only; elsewhere start returns UnsupportedPlatform.

use crate::model::{BackupRequest, CloneError, CloneProgressEvent, CloneSummary};
use crate::clone::CancelToken;

#[cfg(target_os = "linux")]
pub fn run_backup<F>(
    request: &BackupRequest,
    cancel: &CancelToken,
    mut emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    use crate::drives::list_drives;
    use crate::model::PartitionProgress;
    use crate::pipeline::{compression_cmd, compression_suffix, partclone_binary};
    use crate::clone::parse_partclone_fraction;
    use crate::logger::OperationLog;
    use std::fs;
    use std::io::{BufRead, BufReader};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    crate::preflight::require_ready()?;
    let log = OperationLog::open("backup");
    let started = Instant::now();
    let drives = list_drives()?;
    let source = drives
        .iter()
        .find(|d| d.key == request.source_drive_key)
        .ok_or_else(|| CloneError::Other("source drive not found".into()))?;
    log.line(&format!(
        "backup {} -> {} (compression={}, rescue={})",
        request.source_drive_key, request.dest_dir, request.compression, request.rescue
    ));

    let dest = Path::new(&request.dest_dir);
    fs::create_dir_all(dest).map_err(|e| CloneError::Other(format!("mkdir dest: {e}")))?;

    let short = |k: &str| k.trim_start_matches("/dev/").to_string();
    let disk_short = short(&source.key);

    // Which partitions: requested set, else all non-extended, non-swap.
    let selected: Vec<_> = source
        .partitions
        .iter()
        .filter(|p| !p.is_extended)
        .filter(|p| p.filesystem.as_deref() != Some("swap"))
        .filter(|p| {
            request.partitions.is_empty()
                || request.partitions.contains(&short(&p.key))
                || request.partitions.contains(&p.key)
        })
        .collect();
    if selected.is_empty() {
        return Err(CloneError::Other("no partitions selected to back up".into()));
    }

    emit(CloneProgressEvent {
        status: "preparing".into(),
        overall_fraction: 0.0,
        current: None,
        message: "Writing image metadata…".into(),
        log_line: Some(format!("creating image directory {}", request.dest_dir)),
    });

    // --- metadata files ---
    let write = |name: &str, contents: &str| -> Result<(), CloneError> {
        fs::write(dest.join(name), contents)
            .map_err(|e| CloneError::Other(format!("write {name}: {e}")))
    };

    // clonezilla-img marker / log
    write(
        "clonezilla-img",
        &format!("# RescueClone image of {}\n# created {}\n", source.key, now_iso()),
    )?;
    // parts / disk
    let parts_line = selected
        .iter()
        .map(|p| short(&p.key))
        .collect::<Vec<_>>()
        .join(" ");
    write("parts", &format!("{parts_line}\n"))?;
    write("disk", &format!("{disk_short}\n"))?;
    if !request.notes.trim().is_empty() {
        write("rescuezilla.description.txt", &format!("{}\n", request.notes))?;
    }

    // Partition table dump used on restore.
    run_to_file(dest.join(format!("{disk_short}-pt.sf")), "sfdisk", &["--dump", &source.key])?;
    run_to_file(
        dest.join(format!("{disk_short}-pt.parted")),
        "parted",
        &["--script", &source.key, "unit", "s", "print"],
    )
    .ok();
    // MBR (first sector) — used to restore boot code on DOS tables.
    Command::new("dd")
        .args([
            "if=".to_string() + &source.key,
            "of=".to_string() + dest.join(format!("{disk_short}-mbr")).to_str().unwrap(),
            "bs=512".into(),
            "count=1".into(),
        ])
        .stderr(Stdio::null())
        .status()
        .ok();
    // dev-fs.list
    let mut dev_fs = String::from("# <Device name> <File system>\n");
    for p in &source.partitions {
        dev_fs.push_str(&format!("{} {}\n", p.key, p.filesystem.clone().unwrap_or_else(|| "unknown".into())));
    }
    write("dev-fs.list", &dev_fs)?;

    // --- byte-weighted imaging ---
    let weight_of = |p: &crate::model::Partition| p.used_bytes.unwrap_or(p.size_bytes).max(1);
    let total_weight: u64 = selected.iter().map(|p| weight_of(p)).sum::<u64>().max(1);
    let mut completed_weight: u64 = 0;
    let mut lines = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let log_path = Some(log.path().to_string_lossy().to_string());

    for p in &selected {
        if cancel.is_cancelled() {
            cleanup_partials(dest, &selected, short);
            warnings.push("Partial image files were removed after cancellation.".into());
            let s = summary("cancelled", started.elapsed().as_secs(), lines, warnings, log_path.clone());
            log.summary(&s.status, &s.lines, s.duration_seconds);
            return Ok(s);
        }
        let fs = p.filesystem.clone().unwrap_or_default();
        let bin = partclone_binary(&fs);
        let suffix = compression_suffix(&request.compression);
        let kind = if bin == "partclone.dd" { "dd".to_string() } else { fs.clone() };
        let prefix = format!("{}.{}-ptcl-img.{}.", short(&p.key), if kind.is_empty() { "dd".into() } else { kind.clone() }, suffix);

        let cmd = format!(
            "{bin} --clone --source {} --output - | {} | split -b 4GB - {}",
            p.key, request.compression, prefix
        );
        log.command(&cmd);
        emit(CloneProgressEvent {
            status: "copying".into(),
            overall_fraction: completed_weight as f64 / total_weight as f64,
            current: Some(PartitionProgress {
                source_key: p.key.clone(),
                dest_key: prefix.clone(),
                fraction: 0.0,
                status_line: "Starting…".into(),
            }),
            message: format!("Imaging {}", p.key),
            log_line: Some(cmd),
        });

        // partclone --clone --source <part> --output -
        let mut pc_args = vec![];
        if request.rescue {
            pc_args.push("--rescue".to_string());
        }
        if bin == "partclone.dd" {
            pc_args.push("--buffer_size=16777216".into());
        }
        pc_args.extend(["--clone".into(), "--source".into(), p.key.clone(), "--output".into(), "-".into()]);

        let mut partclone = Command::new(&bin)
            .args(&pc_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CloneError::Command(bin.clone(), e.to_string()))?;
        let pc_out = partclone.stdout.take().unwrap();

        let comp = compression_cmd(&request.compression, request.compression_level);
        let mut compressor = Command::new(&comp[0])
            .args(&comp[1..])
            .stdin(Stdio::from(pc_out))
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| CloneError::Command(comp[0].clone(), e.to_string()))?;
        let comp_out = compressor.stdout.take().unwrap();

        let split_prefix = dest.join(&prefix);
        let mut split = Command::new("split")
            .args([
                "--suffix-length=2".to_string(),
                "--bytes=4GB".into(),
                "-".into(),
                split_prefix.to_str().unwrap().to_string(),
            ])
            .stdin(Stdio::from(comp_out))
            .spawn()
            .map_err(|e| CloneError::Command("split".into(), e.to_string()))?;

        // progress from partclone stderr
        let stderr = partclone.stderr.take().unwrap();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if cancel.is_cancelled() {
                let _ = partclone.kill();
                let _ = compressor.kill();
                let _ = split.kill();
                cleanup_partials(dest, &selected, short);
                warnings.push("Partial image files were removed after cancellation.".into());
                let s = summary("cancelled", started.elapsed().as_secs(), lines, warnings, log_path.clone());
                log.summary(&s.status, &s.lines, s.duration_seconds);
                return Ok(s);
            }
            if let Some(frac) = parse_partclone_fraction(&line) {
                let overall = (completed_weight as f64 + weight_of(p) as f64 * frac) / total_weight as f64;
                emit(CloneProgressEvent {
                    status: "copying".into(),
                    overall_fraction: overall,
                    current: Some(PartitionProgress {
                        source_key: p.key.clone(),
                        dest_key: prefix.clone(),
                        fraction: frac,
                        status_line: line.trim().to_string(),
                    }),
                    message: format!("Imaging {}", p.key),
                    log_line: None,
                });
            }
        }

        let pc_status = partclone.wait().map_err(|e| CloneError::Command(bin.clone(), e.to_string()))?;
        let _ = compressor.wait();
        let _ = split.wait();
        if !pc_status.success() {
            if request.rescue {
                let w = format!("{}: imaged with recoverable read errors (rescue mode)", p.key);
                log.line(&format!("WARNING: {w}"));
                warnings.push(w);
                lines.push(format!("{} -> {}*: imaged with errors", p.key, prefix));
            } else {
                lines.push(format!("{}: imaging FAILED", p.key));
                // Remove this partition's partial split files. (5.6)
                remove_prefix(dest, &prefix);
                let s = summary("failed", started.elapsed().as_secs(), lines, warnings, log_path.clone());
                log.summary(&s.status, &s.lines, s.duration_seconds);
                return Ok(s);
            }
        } else {
            lines.push(format!("{} -> {}*: imaged", p.key, prefix));
        }
        completed_weight += weight_of(p);
    }

    let _ = Command::new("sync").status();
    emit(CloneProgressEvent {
        status: "finalizing".into(),
        overall_fraction: 1.0,
        current: None,
        message: "Backup complete".into(),
        log_line: Some("sync".into()),
    });
    let s = summary("completed", started.elapsed().as_secs(), lines, warnings, log_path);
    log.summary(&s.status, &s.lines, s.duration_seconds);
    Ok(s)
}

#[cfg(target_os = "windows")]
pub fn run_backup<F>(
    request: &BackupRequest,
    cancel: &CancelToken,
    mut emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    crate::windows::run_backup_windows(request, cancel, &mut emit)
}

#[cfg(target_os = "macos")]
pub fn run_backup<F>(
    request: &BackupRequest,
    cancel: &CancelToken,
    mut emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    crate::macos::run_backup_macos(request, cancel, &mut emit)
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn run_backup<F>(
    _request: &BackupRequest,
    _cancel: &CancelToken,
    _emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    Err(CloneError::UnsupportedPlatform)
}

#[cfg(target_os = "linux")]
fn now_iso() -> String {
    use std::process::Command;
    String::from_utf8_lossy(
        &Command::new("date")
            .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
            .output()
            .map(|o| o.stdout)
            .unwrap_or_default(),
    )
    .trim()
    .to_string()
}

#[cfg(target_os = "linux")]
fn run_to_file(path: std::path::PathBuf, cmd: &str, args: &[&str]) -> Result<(), CloneError> {
    use std::process::Command;
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| CloneError::Command(cmd.to_string(), e.to_string()))?;
    if !out.status.success() {
        return Err(CloneError::Command(
            cmd.to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ));
    }
    std::fs::write(&path, &out.stdout)
        .map_err(|e| CloneError::Other(format!("write {}: {e}", path.display())))
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
        "completed" => "Backup completed successfully",
        "cancelled" => "Backup cancelled",
        _ => "Backup failed",
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

/// Remove the split files for a single partition prefix. (5.6)
#[cfg(target_os = "linux")]
fn remove_prefix(dest: &std::path::Path, prefix: &str) {
    if let Ok(entries) = std::fs::read_dir(dest) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with(prefix) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// Remove any partial image files written for the selected partitions. (5.6)
#[cfg(target_os = "linux")]
fn cleanup_partials<F>(dest: &std::path::Path, selected: &[&crate::model::Partition], short: F)
where
    F: Fn(&str) -> String,
{
    if let Ok(entries) = std::fs::read_dir(dest) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let is_image = selected.iter().any(|p| name.starts_with(&format!("{}.", short(&p.key))))
                && name.contains("-ptcl-img.");
            if is_image {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}
