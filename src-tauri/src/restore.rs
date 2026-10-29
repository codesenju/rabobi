//! Restore engine + image scanning, reverse-engineered from Rescuezilla's
//! restore_manager.py (non-clone path) and parser/clonezilla_image.py.
//!
//! Scan: a directory is a Clonezilla image iff it contains a `parts` file.
//! Restore:
//!   1. (optional) write partition table: dd MBR boot code, then
//!      `cat <disk>-pt.sf | sfdisk <dest>`
//!   2. per partition: `cat <split parts> | <decompressor> |
//!      partclone.<fs> --restore --overwrite <dest_part>`  (dd for raw)
//!
//! Linux + root only for the restore; scanning works on any platform.

use crate::model::{
    CloneError, CloneProgressEvent, CloneSummary, ImageInfo, ImagePartitionInfo, RestoreRequest,
};
use crate::clone::CancelToken;
use std::fs;
use std::path::Path;

/// Scan a directory tree (one level deep) for Clonezilla image directories.
pub fn scan_images(root: &str) -> Result<Vec<ImageInfo>, CloneError> {
    let root_path = Path::new(root);
    let mut out = Vec::new();

    // The root itself may be an image dir.
    if root_path.join("parts").is_file() {
        if let Some(info) = parse_image_dir(root_path) {
            out.push(info);
        }
    }
    // Otherwise scan immediate children.
    if let Ok(entries) = fs::read_dir(root_path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && p.join("parts").is_file() {
                if let Some(info) = parse_image_dir(&p) {
                    out.push(info);
                }
            }
        }
    }

    // On Windows, also surface native raw images (rescueclone-win-img).
    #[cfg(any(target_os = "windows", test))]
    {
        out.extend(crate::windows::scan_windows_images(root));
    }

    Ok(out)
}

fn parse_image_dir(dir: &Path) -> Option<ImageInfo> {
    let parts_raw = fs::read_to_string(dir.join("parts")).ok()?;
    let part_nodes: Vec<String> = parts_raw.split_whitespace().map(|s| s.to_string()).collect();
    let disk = fs::read_to_string(dir.join("disk"))
        .ok()
        .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
        .unwrap_or_default();
    let notes = fs::read_to_string(dir.join("rescuezilla.description.txt")).unwrap_or_default();

    // last modified from clonezilla-img (or the dir).
    let last_modified = fs::metadata(dir.join("clonezilla-img"))
        .or_else(|_| fs::metadata(dir))
        .ok()
        .and_then(|m| m.modified().ok())
        .map(format_system_time)
        .unwrap_or_else(|| "unknown".into());

    // dev-fs.list → filesystem per device
    let mut dev_fs = std::collections::HashMap::new();
    if let Ok(list) = fs::read_to_string(dir.join("dev-fs.list")) {
        for line in list.lines().filter(|l| !l.starts_with('#')) {
            let mut it = line.split_whitespace();
            if let (Some(dev), Some(fs_)) = (it.next(), it.next()) {
                dev_fs.insert(
                    dev.trim_start_matches("/dev/").to_string(),
                    fs_.to_string(),
                );
            }
        }
    }

    let mut partitions = Vec::new();
    let mut total: u64 = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        let files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        for node in &part_nodes {
            // find split image files for this partition: "<node>.<fs>-ptcl-img.<comp>.aa"
            let matches: Vec<&std::path::PathBuf> = files
                .iter()
                .filter(|f| {
                    f.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with(&format!("{node}.")) && n.contains("-ptcl-img."))
                        .unwrap_or(false)
                })
                .collect();
            let size: u64 = matches
                .iter()
                .filter_map(|f| fs::metadata(f).ok().map(|m| m.len()))
                .sum();
            total += size;
            let (fs_name, compression) = matches
                .first()
                .and_then(|f| f.file_name().and_then(|n| n.to_str()))
                .map(parse_image_filename)
                .unwrap_or((None, "unknown".into()));
            partitions.push(ImagePartitionInfo {
                key: node.clone(),
                filesystem: fs_name.or_else(|| dev_fs.get(node).cloned()),
                compression,
                size_bytes: size,
            });
        }
    }

    Some(ImageInfo {
        path: dir.to_string_lossy().to_string(),
        name: dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        disk,
        last_modified,
        size_bytes: total,
        notes: notes.trim().to_string(),
        partitions,
    })
}

/// "sda1.ext4-ptcl-img.zst.aa" -> (Some("ext4"), "zstd")
fn parse_image_filename(name: &str) -> (Option<String>, String) {
    // <node>.<fs>-ptcl-img.<comp>.<split>
    let after_node = name.split_once('.').map(|x| x.1).unwrap_or(name);
    let fs = after_node.split("-ptcl-img.").next().map(|s| s.to_string());
    let comp = after_node
        .split("-ptcl-img.")
        .nth(1)
        .and_then(|rest| rest.split('.').next())
        .map(|c| match c {
            "gz" => "gzip",
            "zst" => "zstd",
            "bz2" => "bzip2",
            "uncomp" => "uncompressed",
            other => other,
        })
        .unwrap_or("unknown")
        .to_string();
    let fs = fs.filter(|s| s != "dd");
    (fs, comp)
}

fn format_system_time(t: std::time::SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Simple UTC formatting without chrono.
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // 1970-01-01 epoch day count to Y-M-D
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC")
}

/// Public wrapper so the Windows image scanner can reuse the same formatter.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub fn format_system_time_pub(t: std::time::SystemTime) -> String {
    format_system_time(t)
}

// Howard Hinnant's days->civil algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(target_os = "linux")]
pub fn run_restore<F>(
    request: &RestoreRequest,
    cancel: &CancelToken,
    mut emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    use crate::model::PartitionProgress;
    use crate::pipeline::{decompression_cmd, partclone_binary};
    use crate::clone::parse_partclone_fraction;
    use crate::logger::OperationLog;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::time::Instant;

    crate::preflight::require_ready()?;
    let log = OperationLog::open("restore");
    let started = Instant::now();

    // Destination safety guardrail (Milestone 5.3). The source is an image on
    // disk, so there is no same-device case to reject here.
    let drives = crate::drives::list_drives()?;
    crate::safety::enforce_destination(
        &drives,
        &request.destination_drive_key,
        None,
        request.confirm_device.as_deref(),
    )?;

    let dir = Path::new(&request.image_dir);
    let image = parse_image_dir(dir)
        .ok_or_else(|| CloneError::Other("not a valid Clonezilla image directory".into()))?;
    log.line(&format!(
        "restore {} -> {} (overwrite_table={}, rescue={})",
        request.image_dir, request.destination_drive_key, request.overwrite_partition_table, request.rescue
    ));
    let dest = &request.destination_drive_key;
    let dest_node = |num: u32| -> String {
        if dest.chars().last().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            format!("{dest}p{num}")
        } else {
            format!("{dest}{num}")
        }
    };

    // Step 1: partition table.
    if request.overwrite_partition_table {
        emit(CloneProgressEvent {
            status: "preparing".into(),
            overall_fraction: 0.0,
            current: None,
            message: "Writing destination partition table…".into(),
            log_line: Some(format!("cat {}-pt.sf | sfdisk {}", image.disk, dest)),
        });
        let _ = Command::new("sgdisk").args(["--zap-all", dest]).status();
        // MBR boot code
        let mbr = dir.join(format!("{}-mbr", image.disk));
        if mbr.is_file() {
            Command::new("dd")
                .args([
                    format!("if={}", mbr.display()),
                    format!("of={dest}"),
                    "bs=446".into(),
                    "count=1".into(),
                ])
                .stderr(Stdio::null())
                .status()
                .ok();
        }
        // sfdisk < .sf
        let sf = dir.join(format!("{}-pt.sf", image.disk));
        log.command(&format!("cat {} | sfdisk {}", sf.display(), dest));
        let sf_data = fs::read(&sf).map_err(|e| CloneError::Other(format!("read .sf: {e}")))?;
        let mut sfdisk = Command::new("sfdisk")
            .arg(dest)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CloneError::Command("sfdisk".into(), e.to_string()))?;
        use std::io::Write;
        sfdisk.stdin.take().unwrap().write_all(&sf_data).ok();
        if !sfdisk.wait().map(|s| s.success()).unwrap_or(false) {
            return Err(CloneError::Command("sfdisk".into(), "failed writing partition table".into()));
        }
        let _ = Command::new("partprobe").arg(dest).status();
        let _ = Command::new("sync").status();
    }

    // Step 2: per-partition restore, byte-weighted by image file size.
    let total: u64 = image.partitions.iter().map(|p| p.size_bytes.max(1)).sum::<u64>().max(1);
    let mut done: u64 = 0;
    let mut lines = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let log_path = Some(log.path().to_string_lossy().to_string());

    for part in &image.partitions {
        if cancel.is_cancelled() {
            let s = summary("cancelled", started.elapsed().as_secs(), lines, warnings, log_path.clone());
            log.summary(&s.status, &s.lines, s.duration_seconds);
            return Ok(s);
        }
        // split files for this partition, sorted
        let mut globs: Vec<_> = fs::read_dir(dir)
            .map_err(|e| CloneError::Other(e.to_string()))?
            .flatten()
            .map(|e| e.path())
            .filter(|f| {
                f.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with(&format!("{}.", part.key)) && n.contains("-ptcl-img."))
                    .unwrap_or(false)
            })
            .collect();
        globs.sort();
        if globs.is_empty() {
            let w = format!("{}: no image data found; partition left unchanged", part.key);
            log.line(&format!("WARNING: {w}"));
            warnings.push(w);
            lines.push(format!("{}: no image data (skipped)", part.key));
            continue;
        }

        // destination partition number from the source partition number
        let num: u32 = part
            .key
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
            .parse()
            .unwrap_or(1);
        let dkey = request
            .mapping
            .iter()
            .find(|m| m.source_key.trim_start_matches("/dev/") == part.key && m.enabled)
            .map(|m| m.dest_key.clone())
            .unwrap_or_else(|| dest_node(num));

        let fs_name = part.filesystem.clone().unwrap_or_default();
        let bin = partclone_binary(&fs_name);

        let cmd = format!(
            "cat {} part(s) | {} | {bin} --restore --overwrite {dkey}",
            globs.len(),
            part.compression
        );
        log.command(&cmd);
        emit(CloneProgressEvent {
            status: "copying".into(),
            overall_fraction: done as f64 / total as f64,
            current: Some(PartitionProgress {
                source_key: part.key.clone(),
                dest_key: dkey.clone(),
                fraction: 0.0,
                status_line: "Starting…".into(),
            }),
            message: format!("Restoring {} → {}", part.key, dkey),
            log_line: Some(cmd),
        });

        // cat <globs> | decompress | partclone --restore --overwrite <dkey>
        let mut cat = Command::new("cat")
            .args(globs.iter().map(|p| p.as_os_str()))
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| CloneError::Command("cat".into(), e.to_string()))?;
        let cat_out = cat.stdout.take().unwrap();

        let dcmp = decompression_cmd(&part.compression);
        let mut decomp = Command::new(&dcmp[0])
            .args(&dcmp[1..])
            .stdin(Stdio::from(cat_out))
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| CloneError::Command(dcmp[0].clone(), e.to_string()))?;
        let dec_out = decomp.stdout.take().unwrap();

        let mut rst_args = vec![];
        if request.rescue {
            rst_args.push("--rescue".to_string());
        }
        if bin == "partclone.dd" {
            rst_args.push("--buffer_size=16777216".into());
            rst_args.extend(["--source".into(), "-".into(), "--overwrite".into(), dkey.clone()]);
        } else {
            rst_args.extend([
                "--restore".into(),
                "--source".into(),
                "-".into(),
                "--overwrite".into(),
                dkey.clone(),
            ]);
        }
        let mut restore = Command::new(&bin)
            .args(&rst_args)
            .stdin(Stdio::from(dec_out))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CloneError::Command(bin.clone(), e.to_string()))?;

        let stderr = restore.stderr.take().unwrap();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if cancel.is_cancelled() {
                let _ = restore.kill();
                let _ = decomp.kill();
                let _ = cat.kill();
                let s = summary("cancelled", started.elapsed().as_secs(), lines, warnings, log_path.clone());
                log.summary(&s.status, &s.lines, s.duration_seconds);
                return Ok(s);
            }
            if let Some(frac) = parse_partclone_fraction(&line) {
                let overall = (done as f64 + part.size_bytes.max(1) as f64 * frac) / total as f64;
                emit(CloneProgressEvent {
                    status: "copying".into(),
                    overall_fraction: overall,
                    current: Some(PartitionProgress {
                        source_key: part.key.clone(),
                        dest_key: dkey.clone(),
                        fraction: frac,
                        status_line: line.trim().to_string(),
                    }),
                    message: format!("Restoring {} → {}", part.key, dkey),
                    log_line: None,
                });
            }
        }

        let ok = restore.wait().map(|s| s.success()).unwrap_or(false);
        let _ = decomp.wait();
        let _ = cat.wait();
        if !ok {
            if request.rescue {
                let w = format!("{} → {}: restored with recoverable errors (rescue mode)", part.key, dkey);
                log.line(&format!("WARNING: {w}"));
                warnings.push(w);
                lines.push(format!("{} → {}: restored with errors", part.key, dkey));
            } else {
                lines.push(format!("{} → {}: FAILED", part.key, dkey));
                let s = summary("failed", started.elapsed().as_secs(), lines, warnings, log_path.clone());
                log.summary(&s.status, &s.lines, s.duration_seconds);
                return Ok(s);
            }
        } else {
            lines.push(format!("{} → {}: restored", part.key, dkey));
        }

        // Optional post-op verification (5.4): hash the restored device so the
        // log captures an integrity fingerprint and surfaces read failures.
        if request.verify && !cancel.is_cancelled() {
            emit(CloneProgressEvent {
                status: "finalizing".into(),
                overall_fraction: (done + part.size_bytes.max(1)) as f64 / total as f64,
                current: None,
                message: format!("Verifying {dkey} …"),
                log_line: Some(format!("verify: hashing restored {dkey}")),
            });
            match crate::verify::hash_device(&dkey, 0, cancel) {
                Ok(h) => log.line(&format!("verify {dkey} sha256={h}")),
                Err(e) => {
                    let w = format!("verification of {dkey} could not run: {e}");
                    log.line(&format!("WARNING: {w}"));
                    warnings.push(w);
                }
            }
        }
        done += part.size_bytes.max(1);
    }

    let _ = Command::new("sync").status();
    emit(CloneProgressEvent {
        status: "finalizing".into(),
        overall_fraction: 1.0,
        current: None,
        message: "Restore complete".into(),
        log_line: Some("sync".into()),
    });
    let s = summary("completed", started.elapsed().as_secs(), lines, warnings, log_path);
    log.summary(&s.status, &s.lines, s.duration_seconds);
    Ok(s)
}

#[cfg(target_os = "windows")]
pub fn run_restore<F>(
    request: &RestoreRequest,
    cancel: &CancelToken,
    mut emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    crate::windows::run_restore_windows(request, cancel, &mut emit)
}

#[cfg(target_os = "macos")]
pub fn run_restore<F>(
    request: &RestoreRequest,
    cancel: &CancelToken,
    mut emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    crate::macos::run_restore_macos(request, cancel, &mut emit)
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn run_restore<F>(
    _request: &RestoreRequest,
    _cancel: &CancelToken,
    _emit: F,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    Err(CloneError::UnsupportedPlatform)
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
        "completed" => "Restore completed successfully",
        "cancelled" => "Restore cancelled",
        _ => "Restore failed",
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
    fn parse_filename() {
        let (fs, comp) = parse_image_filename("sda1.ext4-ptcl-img.zst.aa");
        assert_eq!(fs.as_deref(), Some("ext4"));
        assert_eq!(comp, "zstd");
        let (fs2, comp2) = parse_image_filename("sda2.dd-ptcl-img.gz.aa");
        assert_eq!(fs2, None);
        assert_eq!(comp2, "gzip");
    }

    #[test]
    fn civil_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }
}
