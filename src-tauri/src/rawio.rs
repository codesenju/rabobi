//! Cross-platform raw-device I/O used by the Windows and macOS engines.
//!
//! These operate on raw device paths (`\\.\PhysicalDriveN` on Windows,
//! `/dev/rdiskN` on macOS) and the Rabobi raw image format shared with
//! `windows.rs` (a `rabobi-img` metadata file plus gzip/uncompressed
//! split parts). gzip is in-process via `flate2`, so no external tools run.
//!
//! Only compiled where a raw engine exists.
#![cfg(any(target_os = "windows", target_os = "macos"))]

use crate::clone::CancelToken;
use crate::logger::OperationLog;
use crate::model::{BackupRequest, CloneError, CloneProgressEvent, CloneSummary, PartitionProgress, RestoreRequest};
use crate::windows::{summary_named, win_image_meta, WinImageMeta, IMAGE_MARKER, IMAGE_PREFIX, PART_BYTES};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs;
use std::io::{Read, Write};
use std::time::Instant;

const CHUNK: usize = 4 * 1024 * 1024;

/// Chunked raw copy of `len` bytes from `src` to `dst`, reporting progress.
pub fn raw_copy(
    src: &str,
    dst: &str,
    len: u64,
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(f64, &str),
) -> Result<u64, CloneError> {
    let mut reader = fs::File::open(src).map_err(|e| CloneError::Other(format!("open source {src}: {e}")))?;
    let mut writer = fs::OpenOptions::new()
        .write(true)
        .open(dst)
        .map_err(|e| CloneError::Other(format!("open destination {dst}: {e}")))?;

    let mut buf = vec![0u8; CHUNK];
    let mut done: u64 = 0;
    let started = Instant::now();
    loop {
        if cancel.is_cancelled() {
            return Ok(done);
        }
        let n = reader.read(&mut buf).map_err(|e| CloneError::Other(format!("read source: {e}")))?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n]).map_err(|e| CloneError::Other(format!("write destination: {e}")))?;
        done += n as u64;
        let frac = if len > 0 { (done as f64 / len as f64).min(1.0) } else { 0.0 };
        let secs = started.elapsed().as_secs().max(1);
        on_progress(frac, &format!("Copied {done} bytes, {} MB/s avg", done / secs / (1024 * 1024)));
    }
    writer.flush().ok();
    Ok(done)
}

/// Full-disk raw image backup to a folder (gzip or uncompressed, 4 GiB parts).
#[allow(clippy::too_many_arguments)]
pub fn run_raw_backup<F>(
    src_raw: &str,
    model: &str,
    size_bytes: u64,
    request: &BackupRequest,
    cancel: &CancelToken,
    emit: &mut F,
    log: &OperationLog,
    started: Instant,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    let dest = std::path::Path::new(&request.dest_dir);
    fs::create_dir_all(dest).map_err(|e| CloneError::Other(format!("mkdir dest: {e}")))?;

    let compress = request.compression != "uncompressed";
    let mut meta = win_image_meta(
        src_raw,
        model,
        size_bytes,
        if compress { "gzip" } else { "uncompressed" },
        &now_iso(),
        &request.notes,
    );
    // Write metadata up front (sha256 is filled in at the end if verifying).
    let write_meta = |m: &crate::windows::WinImageMeta| -> Result<(), CloneError> {
        fs::write(dest.join(IMAGE_MARKER), serde_json::to_string_pretty(m).unwrap_or_default())
            .map_err(|e| CloneError::Other(format!("write metadata: {e}")))
    };
    write_meta(&meta)?;
    log.command(&format!("rawimage {src_raw} -> {}/{IMAGE_PREFIX}", request.dest_dir));

    // When verifying, hash the source as we stream it (no extra pass). (5.4)
    use sha2::{Digest, Sha256};
    let mut hasher = if request.verify { Some(Sha256::new()) } else { None };

    let mut reader = fs::File::open(src_raw).map_err(|e| CloneError::Other(format!("open source {src_raw}: {e}")))?;
    let total = size_bytes.max(1);
    let mut done: u64 = 0;
    let mut part_index: u32 = 0;
    let mut part_written: u64 = 0;
    let mut buf = vec![0u8; CHUNK];
    let mut warnings = Vec::new();
    let mut lines = Vec::new();

    let open_part = |idx: u32| -> Result<fs::File, CloneError> {
        let name = format!("{IMAGE_PREFIX}{idx:03}");
        fs::File::create(dest.join(&name)).map_err(|e| CloneError::Other(format!("create {name}: {e}")))
    };

    enum Sink {
        Gz(GzEncoder<fs::File>),
        Raw(fs::File),
    }
    impl Sink {
        fn write_all(&mut self, b: &[u8]) -> std::io::Result<()> {
            match self {
                Sink::Gz(g) => g.write_all(b),
                Sink::Raw(f) => f.write_all(b),
            }
        }
        fn finish(self) -> std::io::Result<()> {
            match self {
                Sink::Gz(g) => g.finish().map(|_| ()),
                Sink::Raw(mut f) => f.flush(),
            }
        }
    }
    let make_sink = |idx: u32| -> Result<Sink, CloneError> {
        let f = open_part(idx)?;
        Ok(if compress {
            Sink::Gz(GzEncoder::new(f, Compression::new(6)))
        } else {
            Sink::Raw(f)
        })
    };

    let mut sink = make_sink(part_index)?;

    emit(CloneProgressEvent {
        status: "copying".into(),
        overall_fraction: 0.0,
        current: Some(PartitionProgress {
            source_key: src_raw.to_string(),
            dest_key: format!("{IMAGE_PREFIX}000"),
            fraction: 0.0,
            status_line: "Starting image…".into(),
        }),
        message: format!("Imaging {src_raw}"),
        log_line: None,
    });

    loop {
        if cancel.is_cancelled() {
            let _ = sink.finish();
            cleanup_parts(dest);
            warnings.push("Partial image files were removed after cancellation.".into());
            let s = summary_named("Backup", "cancelled", started.elapsed().as_secs(), lines, warnings, Some(log.path().to_string_lossy().to_string()));
            log.summary(&s.status, &s.lines, s.duration_seconds);
            return Ok(s);
        }
        let n = reader.read(&mut buf).map_err(|e| CloneError::Other(format!("read source: {e}")))?;
        if n == 0 {
            break;
        }
        if part_written + n as u64 > PART_BYTES {
            sink.finish().map_err(|e| CloneError::Other(format!("finish part: {e}")))?;
            part_index += 1;
            part_written = 0;
            sink = make_sink(part_index)?;
        }
        sink.write_all(&buf[..n]).map_err(|e| CloneError::Other(format!("write part: {e}")))?;
        if let Some(h) = hasher.as_mut() {
            h.update(&buf[..n]);
        }
        part_written += n as u64;
        done += n as u64;
        let frac = (done as f64 / total as f64).min(1.0);
        let secs = started.elapsed().as_secs().max(1);
        emit(CloneProgressEvent {
            status: "copying".into(),
            overall_fraction: frac,
            current: Some(PartitionProgress {
                source_key: src_raw.to_string(),
                dest_key: format!("{IMAGE_PREFIX}{part_index:03}"),
                fraction: frac,
                status_line: format!("Imaged {done} bytes, {} MB/s avg", done / secs / (1024 * 1024)),
            }),
            message: format!("Imaging {src_raw}"),
            log_line: None,
        });
    }
    sink.finish().map_err(|e| CloneError::Other(format!("finish part: {e}")))?;
    lines.push(format!("{src_raw} -> {}: imaged {done} bytes into {} part(s)", request.dest_dir, part_index + 1));

    // Finalise the integrity digest and record it in the image metadata. (5.4)
    if let Some(h) = hasher.take() {
        let digest = hex(&h.finalize());
        meta.sha256 = Some(digest.clone());
        write_meta(&meta)?;
        log.line(&format!("source sha256={digest}"));
        lines.push(format!("integrity digest recorded (sha256={}…)", &digest[..16.min(digest.len())]));
    }

    let s = summary_named("Backup", "completed", started.elapsed().as_secs(), lines, warnings, Some(log.path().to_string_lossy().to_string()));
    log.summary(&s.status, &s.lines, s.duration_seconds);
    Ok(s)
}

/// Lowercase hex encoding of a byte slice.
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Restore a Rabobi raw image onto a raw destination device.
pub fn run_raw_restore<F>(
    dst_raw: &str,
    request: &RestoreRequest,
    cancel: &CancelToken,
    emit: &mut F,
    log: &OperationLog,
    started: Instant,
) -> Result<CloneSummary, CloneError>
where
    F: FnMut(CloneProgressEvent),
{
    let dir = std::path::Path::new(&request.image_dir);
    let meta_raw = fs::read_to_string(dir.join(IMAGE_MARKER)).map_err(|e| CloneError::Other(format!("read image metadata: {e}")))?;
    let meta: WinImageMeta = serde_json::from_str(&meta_raw).map_err(|e| CloneError::Other(format!("parse image metadata: {e}")))?;

    let mut parts: Vec<std::path::PathBuf> = fs::read_dir(dir)
        .map_err(|e| CloneError::Other(e.to_string()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with(IMAGE_PREFIX)).unwrap_or(false)
        })
        .collect();
    parts.sort();
    if parts.is_empty() {
        return Err(CloneError::Other("image contains no data parts".into()));
    }
    log.command(&format!("rawrestore {} -> {dst_raw}", request.image_dir));

    let mut writer = fs::OpenOptions::new()
        .write(true)
        .open(dst_raw)
        .map_err(|e| CloneError::Other(format!("open destination {dst_raw}: {e}")))?;

    let compress = meta.compression != "uncompressed";
    let total = meta.size_bytes.max(1);
    let mut done: u64 = 0;
    let mut buf = vec![0u8; CHUNK];
    let mut lines = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Hash the decompressed stream as we write, to compare against the image's
    // recorded source digest when verifying. (5.4)
    use sha2::{Digest, Sha256};
    let verify = request.verify && meta.sha256.is_some();
    let mut hasher = if verify { Some(Sha256::new()) } else { None };

    for part in &parts {
        if cancel.is_cancelled() {
            let s = summary_named("Restore", "cancelled", started.elapsed().as_secs(), lines, warnings, Some(log.path().to_string_lossy().to_string()));
            log.summary(&s.status, &s.lines, s.duration_seconds);
            return Ok(s);
        }
        let file = fs::File::open(part).map_err(|e| CloneError::Other(format!("open part {}: {e}", part.display())))?;
        let mut reader: Box<dyn Read> = if compress { Box::new(GzDecoder::new(file)) } else { Box::new(file) };
        loop {
            if cancel.is_cancelled() {
                let s = summary_named("Restore", "cancelled", started.elapsed().as_secs(), lines, warnings, Some(log.path().to_string_lossy().to_string()));
                log.summary(&s.status, &s.lines, s.duration_seconds);
                return Ok(s);
            }
            let n = reader.read(&mut buf).map_err(|e| CloneError::Other(format!("decompress read: {e}")))?;
            if n == 0 {
                break;
            }
            writer.write_all(&buf[..n]).map_err(|e| CloneError::Other(format!("write destination: {e}")))?;
            if let Some(h) = hasher.as_mut() {
                h.update(&buf[..n]);
            }
            done += n as u64;
            let frac = (done as f64 / total as f64).min(1.0);
            emit(CloneProgressEvent {
                status: "copying".into(),
                overall_fraction: frac,
                current: Some(PartitionProgress {
                    source_key: meta.model.clone(),
                    dest_key: dst_raw.to_string(),
                    fraction: frac,
                    status_line: format!("Restored {done} bytes"),
                }),
                message: format!("Restoring image → {dst_raw}"),
                log_line: None,
            });
        }
    }
    writer.flush().ok();
    lines.push(format!("image → {dst_raw}: restored {done} bytes"));

    // Self-check the restored data against the image's recorded digest. (5.4)
    if let (Some(h), Some(expected)) = (hasher.take(), meta.sha256.as_ref()) {
        let actual = hex(&h.finalize());
        if &actual == expected {
            lines.push("verification: OK (restored data matches image digest)".into());
            log.line(&format!("verify OK sha256={actual}"));
        } else {
            let w = format!("verification FAILED: restored sha256={actual} != image sha256={expected}");
            log.line(&format!("WARNING: {w}"));
            warnings.push(w);
        }
    } else if request.verify && meta.sha256.is_none() {
        warnings.push("verify requested but the image has no recorded digest (back up with --verify)".into());
    }

    let s = summary_named("Restore", "completed", started.elapsed().as_secs(), lines, warnings, Some(log.path().to_string_lossy().to_string()));
    log.summary(&s.status, &s.lines, s.duration_seconds);
    Ok(s)
}

fn cleanup_parts(dir: &std::path::Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with(IMAGE_PREFIX) {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Reuse the logger's formatter indirectly via a simple UTC render.
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}
