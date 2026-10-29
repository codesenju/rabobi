//! Windows platform layer.
//!
//! Windows has no partclone/lsblk/sfdisk, so RescueClone uses native Windows
//! facilities:
//!   - enumeration via PowerShell Get-Disk / Get-Partition / Get-Volume
//!   - copying via raw sector reads/writes of \\.\PhysicalDriveN (see rawcopy.rs)
//!
//! The copy path is a full-disk raw (dd-style) copy — it is not filesystem
//! aware, so it copies every sector rather than only used blocks. This is less
//! space-efficient than the Linux partclone path but is correct and needs no
//! third-party tools.

use crate::model::{CloneError, Drive, Partition};

/// PowerShell script that emits a JSON array of disks with nested partitions.
/// Kept here so both enumeration and tests reference the same shape.
#[cfg(target_os = "windows")]
const PS_ENUM: &str = r#"
$disks = Get-Disk | ForEach-Object {
  $disk = $_
  $parts = @()
  try {
    $parts = Get-Partition -DiskNumber $disk.Number -ErrorAction SilentlyContinue | ForEach-Object {
      $p = $_
      $vol = $null
      try { $vol = Get-Volume -Partition $p -ErrorAction SilentlyContinue } catch {}
      [pscustomobject]@{
        number     = $p.PartitionNumber
        size       = [uint64]$p.Size
        type       = "$($p.Type)"
        driveLetter= "$($p.DriveLetter)"
        fileSystem = "$($vol.FileSystem)"
        label      = "$($vol.FileSystemLabel)"
        used       = $(if ($vol -and $vol.Size) { [uint64]($vol.Size - $vol.SizeRemaining) } else { 0 })
        mounted    = [bool]$p.DriveLetter
      }
    }
  } catch {}
  [pscustomobject]@{
    number      = $disk.Number
    path        = "\\.\PhysicalDrive$($disk.Number)"
    model       = "$($disk.FriendlyName)"
    size        = [uint64]$disk.Size
    partitionStyle = "$($disk.PartitionStyle)"
    isBoot      = [bool]$disk.IsBoot
    isSystem    = [bool]$disk.IsSystem
    busType     = "$($disk.BusType)"
    partitions  = @($parts)
  }
}
ConvertTo-Json -InputObject @($disks) -Depth 6
"#;

#[cfg(target_os = "windows")]
pub fn list_drives_windows() -> Result<Vec<Drive>, CloneError> {
    use std::process::Command;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", PS_ENUM])
        .output()
        .map_err(|e| CloneError::Command("powershell Get-Disk".into(), e.to_string()))?;
    if !output.status.success() {
        return Err(CloneError::Command(
            "powershell Get-Disk".into(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    parse_windows_json(&output.stdout)
}

/// Parse the JSON produced by PS_ENUM into the shared Drive model. Platform
/// independent so it can be unit-tested on any OS.
pub fn parse_windows_json(bytes: &[u8]) -> Result<Vec<Drive>, CloneError> {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let json: serde_json::Value = serde_json::from_str(trimmed)
        .map_err(|e| CloneError::Other(format!("parsing Get-Disk json: {e}")))?;
    // ConvertTo-Json emits a bare object for a single disk, else an array.
    let disks: Vec<serde_json::Value> = match json {
        serde_json::Value::Array(a) => a,
        other => vec![other],
    };

    let mut drives = Vec::new();
    for d in disks {
        let number = d.get("number").and_then(|v| v.as_u64()).unwrap_or(0);
        let key = d
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("\\\\.\\PhysicalDrive{number}"));
        let model = d.get("model").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let size_bytes = as_u64(d.get("size"));
        let style = d.get("partitionStyle").and_then(|v| v.as_str()).unwrap_or("");
        let partition_table = match style {
            "GPT" | "gpt" => Some("gpt".to_string()),
            "MBR" | "mbr" => Some("msdos".to_string()),
            _ => None,
        };
        let bus = d.get("busType").and_then(|v| v.as_str()).unwrap_or("");
        let removable = matches!(bus, "USB" | "SD" | "MMC");
        let is_boot = d.get("isBoot").and_then(|v| v.as_bool()).unwrap_or(false);
        let is_system_disk = d.get("isSystem").and_then(|v| v.as_bool()).unwrap_or(false);

        let mut partitions = Vec::new();
        let mut any_mounted = false;
        if let Some(parts) = d.get("partitions").and_then(|v| v.as_array()) {
            for p in parts {
                let pnum = p.get("number").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let drive_letter = p.get("driveLetter").and_then(|v| v.as_str()).unwrap_or("");
                let mounted = p.get("mounted").and_then(|v| v.as_bool()).unwrap_or(!drive_letter.is_empty());
                if mounted {
                    any_mounted = true;
                }
                let fs = p.get("fileSystem").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                let mountpoints = if drive_letter.is_empty() {
                    vec![]
                } else {
                    vec![format!("{drive_letter}:\\")]
                };
                partitions.push(Partition {
                    // On Windows a partition's raw path isn't a simple node; use
                    // the disk path plus a partition suffix for display/mapping.
                    key: format!("{key}:{pnum}"),
                    number: pnum,
                    size_bytes: as_u64(p.get("size")),
                    filesystem: fs.map(|s| s.to_lowercase()),
                    label: p
                        .get("label")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string()),
                    is_extended: false,
                    is_raid_member: false,
                    used_bytes: p.get("used").and_then(as_opt_u64).filter(|&u| u > 0),
                    mountpoints,
                });
            }
        }

        let is_system = is_boot || is_system_disk;
        drives.push(Drive {
            key,
            model,
            size_bytes,
            partition_table,
            removable,
            partitions,
            is_mounted: any_mounted,
            is_system,
        });
    }
    Ok(drives)
}

fn as_u64(v: Option<&serde_json::Value>) -> u64 {
    as_opt_u64(v.unwrap_or(&serde_json::Value::Null)).unwrap_or(0)
}

fn as_opt_u64(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// Windows clone = full-disk raw sector copy from one PhysicalDrive to another.
/// Not filesystem-aware (copies every sector), but needs no third-party tools.
#[cfg(target_os = "windows")]
pub fn run_clone_windows<F>(
    request: &crate::model::CloneRequest,
    cancel: &crate::clone::CancelToken,
    emit: &mut F,
) -> Result<crate::model::CloneSummary, CloneError>
where
    F: FnMut(crate::model::CloneProgressEvent),
{
    use crate::logger::OperationLog;
    use crate::model::{CloneProgressEvent, PartitionProgress};
    use std::time::Instant;

    let log = OperationLog::open("clone");
    let started = Instant::now();
    crate::preflight::require_ready()?;
    let drives = list_drives_windows()?;

    crate::safety::enforce_destination(
        &drives,
        &request.destination_drive_key,
        Some(&request.source_drive_key),
        request.confirm_device.as_deref(),
    )?;

    let src = drives
        .iter()
        .find(|d| d.key == request.source_drive_key)
        .ok_or_else(|| CloneError::Other("source drive not found".into()))?;
    let dst = drives
        .iter()
        .find(|d| d.key == request.destination_drive_key)
        .ok_or_else(|| CloneError::Other("destination drive not found".into()))?;

    if dst.size_bytes < src.size_bytes {
        return Err(CloneError::Other(format!(
            "destination ({} bytes) is smaller than source ({} bytes); raw clone cannot shrink",
            dst.size_bytes, src.size_bytes
        )));
    }

    let cmd = format!("rawcopy {} -> {}", src.key, dst.key);
    log.command(&cmd);
    emit(CloneProgressEvent {
        status: "copying".into(),
        overall_fraction: 0.0,
        current: Some(PartitionProgress {
            source_key: src.key.clone(),
            dest_key: dst.key.clone(),
            fraction: 0.0,
            status_line: "Starting raw copy…".into(),
        }),
        message: format!("Cloning {} → {}", src.key, dst.key),
        log_line: Some(cmd),
    });

    let copied = crate::rawio::raw_copy(&src.key, &dst.key, src.size_bytes, cancel, &mut |frac, line| {
        emit(CloneProgressEvent {
            status: "copying".into(),
            overall_fraction: frac,
            current: Some(PartitionProgress {
                source_key: src.key.clone(),
                dest_key: dst.key.clone(),
                fraction: frac,
                status_line: line.to_string(),
            }),
            message: format!("Cloning {} → {}", src.key, dst.key),
            log_line: None,
        });
    });

    let mut warnings = Vec::new();
    let mut lines = Vec::new();
    let status = match copied {
        Ok(_) if cancel.is_cancelled() => "cancelled",
        Ok(bytes) => {
            lines.push(format!("{} → {}: copied {} bytes", src.key, dst.key, bytes));
            if request.verify {
                emit(CloneProgressEvent {
                    status: "finalizing".into(),
                    overall_fraction: 1.0,
                    current: None,
                    message: "Verifying…".into(),
                    log_line: Some("verify: sha256(source) vs sha256(dest)".into()),
                });
                match crate::verify::compare_devices(&src.key, &dst.key, src.size_bytes, cancel) {
                    Ok(true) => lines.push("verification: OK (hashes match)".into()),
                    Ok(false) => {
                        warnings.push("verification FAILED: source and destination differ".into())
                    }
                    Err(e) => warnings.push(format!("verification could not run: {e}")),
                }
            }
            "completed"
        }
        Err(e) => {
            lines.push(format!("{} → {}: FAILED: {e}", src.key, dst.key));
            "failed"
        }
    };

    let s = build_summary(status, started.elapsed().as_secs(), lines, warnings, Some(log.path().to_string_lossy().to_string()));
    log.summary(&s.status, &s.lines, s.duration_seconds);
    Ok(s)
}

/// Chunked raw copy of `len` bytes from `src` to `dst`, reporting progress.
/// Shared summary builder (also used by the Linux engines via re-export).
pub fn build_summary(
    status: &str,
    duration: u64,
    lines: Vec<String>,
    warnings: Vec<String>,
    log_path: Option<String>,
) -> crate::model::CloneSummary {
    let noun = "Clone";
    let title = match status {
        "completed" => format!("{noun} completed successfully"),
        "cancelled" => format!("{noun} cancelled"),
        _ => format!("{noun} failed"),
    };
    let mut lines = lines;
    if status == "cancelled" {
        lines.push("Operation cancelled by user.".into());
    }
    crate::model::CloneSummary {
        status: status.to_string(),
        title,
        lines,
        duration_seconds: duration,
        warnings,
        log_path,
    }
}

// ---------------------------------------------------------------------------
// Windows raw image backup / restore (Milestone 6.6)
//
// A raw image is a directory containing:
//   rabobi-img                     JSON metadata (model, size, created, notes…)
//   disk.raw-img.gz.000, .001…     gzip-compressed raw disk, split into 4 GiB parts
//
// It is a full-disk raw image (every sector), analogous to the Windows clone.
// ---------------------------------------------------------------------------

/// Chunk size for split image parts (bytes). 4 GiB like the Linux path.
pub const PART_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const IMAGE_MARKER: &str = "rabobi-img";
pub const IMAGE_PREFIX: &str = "disk.raw-img.gz.";

/// Metadata written as JSON into `rabobi-img`. Platform-independent so
/// the scanner (and tests) can read it anywhere.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WinImageMeta {
    pub format: String,     // "rabobi-raw-v1"
    pub source_path: String,
    pub model: String,
    pub size_bytes: u64,
    pub compression: String, // "gzip" | "uncompressed"
    pub created: String,
    pub notes: String,
    /// SHA-256 of the source device (uncompressed bytes), if `--verify` was
    /// used during backup. Lets restore self-check the image. (5.4 / 6.6)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Build the metadata value (shared by backup + tests).
pub fn win_image_meta(
    source_path: &str,
    model: &str,
    size_bytes: u64,
    compression: &str,
    created: &str,
    notes: &str,
) -> WinImageMeta {
    WinImageMeta {
        format: "rabobi-raw-v1".into(),
        source_path: source_path.into(),
        model: model.into(),
        size_bytes,
        compression: compression.into(),
        created: created.into(),
        notes: notes.into(),
        sha256: None,
    }
}

/// Scan a directory (one level deep) for Windows raw images, returning the
/// shared ImageInfo model used by the restore picker. Platform-independent.
pub fn scan_windows_images(root: &str) -> Vec<crate::model::ImageInfo> {
    use std::path::Path;
    let mut out = Vec::new();
    let root_path = Path::new(root);

    let mut consider = |dir: &Path| {
        if let Some(info) = read_windows_image(dir) {
            out.push(info);
        }
    };

    if root_path.join(IMAGE_MARKER).is_file() {
        consider(root_path);
    }
    if let Ok(entries) = std::fs::read_dir(root_path) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && p.join(IMAGE_MARKER).is_file() {
                consider(&p);
            }
        }
    }
    out
}

fn read_windows_image(dir: &std::path::Path) -> Option<crate::model::ImageInfo> {
    let meta_raw = std::fs::read_to_string(dir.join(IMAGE_MARKER)).ok()?;
    let meta: WinImageMeta = serde_json::from_str(&meta_raw).ok()?;
    // Sum split part sizes.
    let mut on_disk: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with(IMAGE_PREFIX) {
                on_disk += std::fs::metadata(e.path()).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    let last_modified = std::fs::metadata(dir.join(IMAGE_MARKER))
        .ok()
        .and_then(|m| m.modified().ok())
        .map(crate::restore::format_system_time_pub)
        .unwrap_or_else(|| meta.created.clone());

    Some(crate::model::ImageInfo {
        path: dir.to_string_lossy().to_string(),
        name: dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        disk: meta.model.clone(),
        last_modified,
        size_bytes: on_disk,
        notes: meta.notes.clone(),
        partitions: vec![crate::model::ImagePartitionInfo {
            key: "whole-disk".into(),
            filesystem: None,
            compression: meta.compression.clone(),
            size_bytes: meta.size_bytes,
        }],
    })
}

#[cfg(target_os = "windows")]
pub fn run_backup_windows<F>(
    request: &crate::model::BackupRequest,
    cancel: &crate::clone::CancelToken,
    emit: &mut F,
) -> Result<crate::model::CloneSummary, CloneError>
where
    F: FnMut(crate::model::CloneProgressEvent),
{
    use crate::logger::OperationLog;
    let log = OperationLog::open("backup");
    let started = std::time::Instant::now();
    crate::preflight::require_ready()?;
    let drives = list_drives_windows()?;
    let src = drives
        .iter()
        .find(|d| d.key == request.source_drive_key)
        .ok_or_else(|| CloneError::Other("source drive not found".into()))?;
    crate::rawio::run_raw_backup(&src.key, &src.model, src.size_bytes, request, cancel, emit, &log, started)
}

#[cfg(target_os = "windows")]
pub fn run_restore_windows<F>(
    request: &crate::model::RestoreRequest,
    cancel: &crate::clone::CancelToken,
    emit: &mut F,
) -> Result<crate::model::CloneSummary, CloneError>
where
    F: FnMut(crate::model::CloneProgressEvent),
{
    use crate::logger::OperationLog;
    let log = OperationLog::open("restore");
    let started = std::time::Instant::now();
    crate::preflight::require_ready()?;
    let drives = list_drives_windows()?;
    crate::safety::enforce_destination(
        &drives,
        &request.destination_drive_key,
        None,
        request.confirm_device.as_deref(),
    )?;
    let dst = drives
        .iter()
        .find(|d| d.key == request.destination_drive_key)
        .ok_or_else(|| CloneError::Other("destination drive not found".into()))?;
    crate::rawio::run_raw_restore(&dst.key, request, cancel, emit, &log, started)
}

/// Summary builder that lets the Windows backup/restore name their noun.
pub fn summary_named(
    noun: &str,
    status: &str,
    duration: u64,
    lines: Vec<String>,
    warnings: Vec<String>,
    log_path: Option<String>,
) -> crate::model::CloneSummary {
    let title = match status {
        "completed" => format!("{noun} completed successfully"),
        "cancelled" => format!("{noun} cancelled"),
        _ => format!("{noun} failed"),
    };
    let mut lines = lines;
    if status == "cancelled" {
        lines.push("Operation cancelled by user.".into());
    }
    crate::model::CloneSummary {
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

    const SAMPLE: &str = r#"[
      {"number":0,"path":"\\\\.\\PhysicalDrive0","model":"Samsung SSD","size":"500107862016","partitionStyle":"GPT","isBoot":true,"isSystem":true,"busType":"NVMe",
       "partitions":[
         {"number":1,"size":"104857600","type":"System","driveLetter":"","fileSystem":"FAT32","label":"","used":"30000000","mounted":false},
         {"number":2,"size":"499002359808","type":"Basic","driveLetter":"C","fileSystem":"NTFS","label":"OS","used":"120000000000","mounted":true}
       ]},
      {"number":1,"path":"\\\\.\\PhysicalDrive1","model":"SanDisk USB","size":"61530439680","partitionStyle":"MBR","isBoot":false,"isSystem":false,"busType":"USB",
       "partitions":[
         {"number":1,"size":"61530439680","type":"Basic","driveLetter":"E","fileSystem":"exFAT","label":"STICK","used":"2000000000","mounted":true}
       ]}
    ]"#;

    #[test]
    fn parses_windows_disks() {
        let drives = parse_windows_json(SAMPLE.as_bytes()).unwrap();
        assert_eq!(drives.len(), 2);
        let d0 = &drives[0];
        assert_eq!(d0.key, "\\\\.\\PhysicalDrive0");
        assert_eq!(d0.partition_table.as_deref(), Some("gpt"));
        assert!(d0.is_system, "boot/system disk");
        assert!(d0.is_mounted);
        assert_eq!(d0.partitions.len(), 2);
        assert_eq!(d0.partitions[1].filesystem.as_deref(), Some("ntfs"));
        assert_eq!(d0.partitions[1].mountpoints, vec!["C:\\".to_string()]);
    }

    #[test]
    fn usb_is_removable_not_system() {
        let drives = parse_windows_json(SAMPLE.as_bytes()).unwrap();
        let usb = &drives[1];
        assert!(usb.removable);
        assert!(!usb.is_system);
        assert_eq!(usb.partition_table.as_deref(), Some("msdos"));
    }

    #[test]
    fn single_object_is_wrapped() {
        let one = r#"{"number":0,"path":"\\\\.\\PhysicalDrive0","model":"X","size":"1024","partitionStyle":"RAW","isBoot":false,"isSystem":false,"busType":"SATA","partitions":[]}"#;
        let drives = parse_windows_json(one.as_bytes()).unwrap();
        assert_eq!(drives.len(), 1);
        assert_eq!(drives[0].partition_table, None);
    }

    #[test]
    fn scans_a_written_windows_image() {
        let base = std::env::temp_dir().join(format!("rc-winimg-{}", std::process::id()));
        let dir = base.join("2026-01-01-img-win");
        std::fs::create_dir_all(&dir).unwrap();
        let meta = win_image_meta(
            "\\\\.\\PhysicalDrive0",
            "Samsung SSD",
            500_000_000_000,
            "gzip",
            "2026-01-01T10:00:00Z",
            "nightly image",
        );
        std::fs::write(dir.join(IMAGE_MARKER), serde_json::to_string(&meta).unwrap()).unwrap();
        std::fs::write(dir.join(format!("{IMAGE_PREFIX}000")), vec![0u8; 1024]).unwrap();
        std::fs::write(dir.join(format!("{IMAGE_PREFIX}001")), vec![0u8; 512]).unwrap();

        let imgs = scan_windows_images(base.to_str().unwrap());
        assert_eq!(imgs.len(), 1);
        let img = &imgs[0];
        assert_eq!(img.name, "2026-01-01-img-win");
        assert_eq!(img.disk, "Samsung SSD");
        assert_eq!(img.notes, "nightly image");
        assert_eq!(img.size_bytes, 1024 + 512); // on-disk size of parts
        assert_eq!(img.partitions.len(), 1);
        assert_eq!(img.partitions[0].compression, "gzip");
        assert_eq!(img.partitions[0].size_bytes, 500_000_000_000);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn meta_roundtrips_through_json() {
        let meta = win_image_meta("\\\\.\\PhysicalDrive1", "USB", 64_000_000_000, "uncompressed", "2026-02-02T00:00:00Z", "");
        let json = serde_json::to_string(&meta).unwrap();
        let back: WinImageMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back.format, "rabobi-raw-v1");
        assert_eq!(back.size_bytes, 64_000_000_000);
        assert_eq!(back.compression, "uncompressed");
        assert!(back.sha256.is_none());
        // Absent digest is not serialised.
        assert!(!json.contains("sha256"));
    }

    #[test]
    fn meta_carries_sha256_when_present() {
        let mut meta = win_image_meta("/dev/rdisk4", "Cruzer", 7_849_115_648, "gzip", "2026-10-29T11:00:00Z", "live test");
        meta.sha256 = Some("a".repeat(64));
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("sha256"));
        let back: WinImageMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sha256.as_deref(), Some("a".repeat(64).as_str()));
    }
}
