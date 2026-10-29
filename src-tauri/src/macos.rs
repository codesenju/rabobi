//! macOS platform layer.
//!
//! macOS has no partclone/lsblk/sfdisk either, so RescueClone uses native macOS
//! facilities:
//!   - enumeration via `diskutil list -plist` + `diskutil info -plist <disk>`
//!     (converted to JSON with `plutil` so we can parse without a plist crate)
//!   - copying via raw sector reads/writes of the raw char device `/dev/rdiskN`
//!     (the buffered `/dev/diskN` works too but `rdiskN` is far faster)
//!
//! Like the Windows path this is a full-disk raw (dd-style) copy — not
//! filesystem aware. Destinations must be unmounted first (`diskutil
//! unmountDisk`) and the whole-disk image is captured/restored.

use crate::model::{CloneError, Drive, Partition};

/// Convert a device identifier like "disk4" or "/dev/disk4" into the raw
/// character-device path "/dev/rdisk4" used for fast whole-disk I/O.
pub fn raw_device_path(key: &str) -> String {
    let id = key.trim_start_matches("/dev/").trim_start_matches('r');
    format!("/dev/r{id}")
}

/// The plain block-device path "/dev/disk4" (used for unmount bookkeeping).
pub fn block_device_path(key: &str) -> String {
    let id = key.trim_start_matches("/dev/").trim_start_matches('r');
    format!("/dev/{id}")
}

#[cfg(target_os = "macos")]
pub fn list_drives_macos() -> Result<Vec<Drive>, CloneError> {
    // `diskutil list -plist physical` → the disk/partition tree.
    let list_json = run_plutil_json(&["list", "-plist", "physical"])?;
    let tree: serde_json::Value = serde_json::from_slice(&list_json)
        .map_err(|e| CloneError::Other(format!("parse diskutil list json: {e}")))?;

    let whole_disks: Vec<String> = tree
        .get("AllDisksAndPartitions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|d| d.get("DeviceIdentifier").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Fetch per-disk info JSON (model, bus, removable, internal) for each disk.
    let info_map: serde_json::Map<String, serde_json::Value> = whole_disks
        .iter()
        .filter_map(|id| {
            run_plutil_json(&["info", "-plist", id])
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .map(|v| (id.clone(), v))
        })
        .collect();

    parse_macos_json(&list_json, &serde_json::Value::Object(info_map))
}

/// Run `diskutil <args> | plutil -convert json` and return the JSON bytes.
#[cfg(target_os = "macos")]
fn run_plutil_json(args: &[&str]) -> Result<Vec<u8>, CloneError> {
    use std::process::{Command, Stdio};
    let diskutil = Command::new("diskutil")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CloneError::Command("diskutil".into(), e.to_string()))?;
    let out = Command::new("plutil")
        .args(["-convert", "json", "-o", "-", "-"])
        .stdin(Stdio::from(diskutil.stdout.unwrap()))
        .output()
        .map_err(|e| CloneError::Command("plutil".into(), e.to_string()))?;
    if !out.status.success() {
        return Err(CloneError::Command(
            "plutil".into(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ));
    }
    Ok(out.stdout)
}

/// Parse `diskutil list -plist` JSON (the disk/partition tree) plus a map of
/// per-disk `diskutil info -plist` JSON, into the shared Drive model.
/// Platform-independent so it can be unit-tested anywhere.
pub fn parse_macos_json(
    list_bytes: &[u8],
    info_map: &serde_json::Value,
) -> Result<Vec<Drive>, CloneError> {
    let tree: serde_json::Value = serde_json::from_slice(list_bytes)
        .map_err(|e| CloneError::Other(format!("parse diskutil list json: {e}")))?;

    let disks = tree
        .get("AllDisksAndPartitions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut drives = Vec::new();
    for d in disks {
        let id = d.get("DeviceIdentifier").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let info = info_map.get(id);
        let get_info_str = |k: &str| info.and_then(|i| i.get(k)).and_then(|v| v.as_str());
        let get_info_bool = |k: &str| info.and_then(|i| i.get(k)).and_then(|v| v.as_bool());
        let get_info_u64 = |k: &str| info.and_then(|i| i.get(k)).and_then(as_opt_u64);

        let key = format!("/dev/{id}");
        let model = get_info_str("MediaName").unwrap_or("").trim().to_string();
        let size_bytes = get_info_u64("Size")
            .or_else(|| d.get("Size").and_then(as_opt_u64))
            .unwrap_or(0);
        let content = d.get("Content").and_then(|v| v.as_str()).unwrap_or("");
        let partition_table = match content {
            "GUID_partition_scheme" => Some("gpt".to_string()),
            "FDisk_partition_scheme" => Some("msdos".to_string()),
            _ => None,
        };
        let bus = get_info_str("BusProtocol").unwrap_or("");
        let removable = get_info_bool("Removable").unwrap_or(false) || bus == "USB";
        // A disk is "system" if it is the internal media (hosts macOS).
        let is_system = get_info_bool("Internal").unwrap_or(false);

        let mut partitions = Vec::new();
        let mut any_mounted = false;
        if let Some(parts) = d.get("Partitions").and_then(|v| v.as_array()) {
            for (idx, p) in parts.iter().enumerate() {
                let pid = p.get("DeviceIdentifier").and_then(|v| v.as_str()).unwrap_or("");
                let number = pid
                    .rsplit('s')
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or((idx + 1) as u32);
                let mount = p.get("MountPoint").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                if mount.is_some() {
                    any_mounted = true;
                }
                let content = p.get("Content").and_then(|v| v.as_str()).unwrap_or("");
                partitions.push(Partition {
                    key: format!("/dev/{pid}"),
                    number,
                    size_bytes: p.get("Size").and_then(as_opt_u64).unwrap_or(0),
                    filesystem: macos_content_to_fs(content),
                    label: p
                        .get("VolumeName")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string()),
                    is_extended: false,
                    is_raid_member: false,
                    used_bytes: None,
                    mountpoints: mount.map(|m| vec![m.to_string()]).unwrap_or_default(),
                });
            }
        }

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

/// Map a macOS partition "Content" string to a rough filesystem name.
fn macos_content_to_fs(content: &str) -> Option<String> {
    let fs = match content {
        "Apple_HFS" => "hfsplus",
        "Apple_APFS" => "apfs",
        "EFI" => "fat32",
        "Microsoft Basic Data" => "ntfs-or-fat",
        "Linux Filesystem" => "ext4",
        "" => return None,
        other => other,
    };
    Some(fs.to_string())
}

fn as_opt_u64(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

// ---------------------------------------------------------------------------
// macOS raw clone / backup / restore (reuses the Windows image format)
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn unmount_disk(key: &str) -> Result<(), CloneError> {
    use std::process::Command;
    let block = block_device_path(key);
    // Best-effort: ignore "already unmounted"; fail loudly only on hard errors.
    let out = Command::new("diskutil")
        .args(["unmountDisk", "force", &block])
        .output()
        .map_err(|e| CloneError::Command("diskutil unmountDisk".into(), e.to_string()))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        // "unmount failed" on a disk with no mounted volumes is fine.
        if !msg.contains("was already unmounted") && !msg.trim().is_empty() {
            return Err(CloneError::Command(
                "diskutil unmountDisk".into(),
                msg.to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn run_clone_macos<F>(
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
    let drives = list_drives_macos()?;

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

    // Both disks must be unmounted for raw access.
    log.command(&format!("diskutil unmountDisk force {}", block_device_path(&src.key)));
    unmount_disk(&src.key)?;
    log.command(&format!("diskutil unmountDisk force {}", block_device_path(&dst.key)));
    unmount_disk(&dst.key)?;

    let src_raw = raw_device_path(&src.key);
    let dst_raw = raw_device_path(&dst.key);
    let cmd = format!("rawcopy {src_raw} -> {dst_raw}");
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

    let mut warnings = Vec::new();
    let mut lines = Vec::new();
    let copied = crate::rawio::raw_copy(&src_raw, &dst_raw, src.size_bytes, cancel, &mut |frac, line| {
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
                match crate::verify::compare_devices(&src_raw, &dst_raw, src.size_bytes, cancel) {
                    Ok(true) => lines.push("verification: OK (hashes match)".into()),
                    Ok(false) => warnings.push("verification FAILED: source and destination differ".into()),
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

    let s = crate::windows::summary_named(
        "Clone",
        status,
        started.elapsed().as_secs(),
        lines,
        warnings,
        Some(log.path().to_string_lossy().to_string()),
    );
    log.summary(&s.status, &s.lines, s.duration_seconds);
    Ok(s)
}

#[cfg(target_os = "macos")]
pub fn run_backup_macos<F>(
    request: &crate::model::BackupRequest,
    cancel: &crate::clone::CancelToken,
    emit: &mut F,
) -> Result<crate::model::CloneSummary, CloneError>
where
    F: FnMut(crate::model::CloneProgressEvent),
{
    use crate::logger::OperationLog;
    use std::time::Instant;

    let log = OperationLog::open("backup");
    let started = Instant::now();
    crate::preflight::require_ready()?;
    let drives = list_drives_macos()?;
    let src = drives
        .iter()
        .find(|d| d.key == request.source_drive_key)
        .ok_or_else(|| CloneError::Other("source drive not found".into()))?;

    unmount_disk(&src.key)?;
    let src_raw = raw_device_path(&src.key);
    crate::rawio::run_raw_backup(
        &src_raw,
        &src.model,
        src.size_bytes,
        request,
        cancel,
        emit,
        &log,
        started,
    )
}

#[cfg(target_os = "macos")]
pub fn run_restore_macos<F>(
    request: &crate::model::RestoreRequest,
    cancel: &crate::clone::CancelToken,
    emit: &mut F,
) -> Result<crate::model::CloneSummary, CloneError>
where
    F: FnMut(crate::model::CloneProgressEvent),
{
    use crate::logger::OperationLog;
    use std::time::Instant;

    let log = OperationLog::open("restore");
    let started = Instant::now();
    crate::preflight::require_ready()?;
    let drives = list_drives_macos()?;
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

    unmount_disk(&dst.key)?;
    let dst_raw = raw_device_path(&dst.key);
    crate::rawio::run_raw_restore(&dst_raw, request, cancel, emit, &log, started)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"{
      "AllDisksAndPartitions": [
        {
          "Content": "GUID_partition_scheme",
          "DeviceIdentifier": "disk4",
          "Size": 7849115648,
          "Partitions": [
            {"Content":"Microsoft Basic Data","DeviceIdentifier":"disk4s1","Size":247808},
            {"Content":"EFI","DeviceIdentifier":"disk4s2","Size":8388608,"VolumeName":"NO NAME"},
            {"Content":"Apple_HFS","DeviceIdentifier":"disk4s3","Size":1440047104,"VolumeName":"PVE","MountPoint":"/Volumes/PVE"},
            {"Content":"Microsoft Basic Data","DeviceIdentifier":"disk4s4","Size":307200}
          ]
        }
      ]
    }"#;

    const INFO: &str = r#"{
      "disk4": {"MediaName":"Cruzer Blade","BusProtocol":"USB","Internal":false,"Removable":true,"Size":7849115648}
    }"#;

    #[test]
    fn parses_macos_disk_and_partitions() {
        let info: serde_json::Value = serde_json::from_str(INFO).unwrap();
        let drives = parse_macos_json(LIST.as_bytes(), &info).unwrap();
        assert_eq!(drives.len(), 1);
        let d = &drives[0];
        assert_eq!(d.key, "/dev/disk4");
        assert_eq!(d.model, "Cruzer Blade");
        assert_eq!(d.size_bytes, 7849115648);
        assert_eq!(d.partition_table.as_deref(), Some("gpt"));
        assert!(d.removable);
        assert!(!d.is_system);
        assert!(d.is_mounted, "PVE is mounted at /Volumes/PVE");
        assert_eq!(d.partitions.len(), 4);
        assert_eq!(d.partitions[2].number, 3);
        assert_eq!(d.partitions[2].label.as_deref(), Some("PVE"));
        assert_eq!(d.partitions[2].mountpoints, vec!["/Volumes/PVE".to_string()]);
    }

    #[test]
    fn raw_and_block_paths() {
        assert_eq!(raw_device_path("/dev/disk4"), "/dev/rdisk4");
        assert_eq!(raw_device_path("disk5"), "/dev/rdisk5");
        assert_eq!(raw_device_path("/dev/rdisk4"), "/dev/rdisk4");
        assert_eq!(block_device_path("/dev/rdisk4"), "/dev/disk4");
        assert_eq!(block_device_path("disk5"), "/dev/disk5");
    }

    #[test]
    fn internal_disk_is_system() {
        let info: serde_json::Value = serde_json::from_str(
            r#"{"disk0":{"MediaName":"APPLE SSD","BusProtocol":"PCI-Express","Internal":true,"Removable":false,"Size":512000000000}}"#,
        )
        .unwrap();
        let list = r#"{"AllDisksAndPartitions":[{"Content":"GUID_partition_scheme","DeviceIdentifier":"disk0","Size":512000000000,"Partitions":[]}]}"#;
        let drives = parse_macos_json(list.as_bytes(), &info).unwrap();
        assert!(drives[0].is_system);
        assert!(!drives[0].removable);
    }
}
