//! Drive enumeration, reverse-engineered from Rescuezilla's drive_query.py +
//! combined_drive_state.py. On Linux it parses `lsblk --json --bytes` to build
//! the Drive/Partition model (the same data lsblk feeds Rescuezilla). On other
//! platforms it returns UnsupportedPlatform so the frontend uses its simulation.

use crate::model::{CloneError, Drive, Partition};

/// Parse the JSON produced by:
///   lsblk --json --bytes -o NAME,PATH,TYPE,SIZE,MODEL,RM,PTTYPE,FSTYPE,LABEL,FSUSED,PARTTYPE
/// Kept platform-independent so it can be unit tested anywhere.
pub fn parse_lsblk_json(bytes: &[u8]) -> Result<Vec<Drive>, CloneError> {
    let json: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| CloneError::Other(format!("parsing lsblk json: {e}")))?;

    let mut drives = Vec::new();
    let blockdevices = json
        .get("blockdevices")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for dev in blockdevices {
        let dev_type = dev.get("type").and_then(|v| v.as_str()).unwrap_or("");
        // Only physical disks and loop devices (loop lets us test safely).
        if dev_type != "disk" && dev_type != "loop" {
            continue;
        }
        let key = dev.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let model = dev
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let size_bytes = as_u64(dev.get("size"));
        let removable = dev
            .get("rm")
            .map(|v| v.as_bool().unwrap_or(false) || v.as_str() == Some("1"))
            .unwrap_or(false);
        let partition_table = dev
            .get("pttype")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let mut partitions = Vec::new();
        if let Some(children) = dev.get("children").and_then(|v| v.as_array()) {
            for (idx, part) in children.iter().enumerate() {
                let pkey = part.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let number = trailing_number(&pkey).unwrap_or((idx + 1) as u32);
                let parttype = part.get("parttype").and_then(|v| v.as_str()).unwrap_or("");
                let fstype = part.get("fstype").and_then(|v| v.as_str());
                let mountpoints = parse_mountpoints(part);
                partitions.push(Partition {
                    key: pkey,
                    number,
                    size_bytes: as_u64(part.get("size")),
                    filesystem: fstype.map(|s| s.to_string()),
                    label: part.get("label").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    is_extended: matches!(parttype, "0x5" | "0x05" | "0xf" | "0x0f"),
                    is_raid_member: fstype == Some("linux_raid_member"),
                    used_bytes: part.get("fsused").and_then(as_opt_u64),
                    mountpoints,
                });
            }
        }

        // Also honour a mountpoint on the whole-disk node (no partition table).
        let disk_mounts = parse_mountpoints(&dev);
        let all_mounts: Vec<&String> = partitions
            .iter()
            .flat_map(|p| p.mountpoints.iter())
            .chain(disk_mounts.iter())
            .collect();
        let is_mounted = !all_mounts.is_empty();
        let is_system = all_mounts.iter().any(|m| is_system_mountpoint(m))
            || partitions.iter().any(|p| p.filesystem.as_deref() == Some("swap") && !p.mountpoints.is_empty());

        drives.push(Drive {
            key,
            model,
            size_bytes,
            partition_table,
            removable,
            partitions,
            is_mounted,
            is_system,
        });
    }

    Ok(drives)
}

/// Extract mountpoints from an lsblk node, handling both the modern
/// MOUNTPOINTS array and the legacy MOUNTPOINT scalar.
fn parse_mountpoints(node: &serde_json::Value) -> Vec<String> {
    if let Some(arr) = node.get("mountpoints").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
    }
    node.get("mountpoint")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| vec![s.to_string()])
        .unwrap_or_default()
}

/// A mountpoint that belongs to the running OS — never safe to overwrite.
fn is_system_mountpoint(m: &str) -> bool {
    const CRITICAL: &[&str] = &["/", "/boot", "/boot/efi", "/usr", "/var", "/etc", "/home"];
    if m == "[SWAP]" {
        return true;
    }
    CRITICAL.contains(&m)
}

/// lsblk may emit sizes as either numbers or strings depending on version.
fn as_u64(v: Option<&serde_json::Value>) -> u64 {
    as_opt_u64(v.unwrap_or(&serde_json::Value::Null)).unwrap_or(0)
}

fn as_opt_u64(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn trailing_number(key: &str) -> Option<u32> {
    let digits: String = key.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
    digits.chars().rev().collect::<String>().parse().ok()
}

#[cfg(target_os = "linux")]
pub fn list_drives() -> Result<Vec<Drive>, CloneError> {
    use std::process::Command;

    let output = Command::new("lsblk")
        .args([
            "--json",
            "--bytes",
            "-o",
            "NAME,PATH,TYPE,SIZE,MODEL,RM,PTTYPE,FSTYPE,LABEL,FSUSED,PARTTYPE,MOUNTPOINTS",
        ])
        .output()
        .map_err(|e| CloneError::Command("lsblk".into(), e.to_string()))?;

    if !output.status.success() {
        return Err(CloneError::Command(
            "lsblk".into(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    parse_lsblk_json(&output.stdout)
}

#[cfg(target_os = "windows")]
pub fn list_drives() -> Result<Vec<Drive>, CloneError> {
    crate::windows::list_drives_windows()
}

#[cfg(target_os = "macos")]
pub fn list_drives() -> Result<Vec<Drive>, CloneError> {
    crate::macos::list_drives_macos()
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn list_drives() -> Result<Vec<Drive>, CloneError> {
    Err(CloneError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "blockdevices": [
        {"name":"sda","path":"/dev/sda","type":"disk","size":500107862016,"model":"Samsung SSD","rm":false,"pttype":"gpt","fstype":null,"label":null,"fsused":null,"parttype":null,"mountpoints":[null],
         "children":[
           {"name":"sda1","path":"/dev/sda1","type":"part","size":536870912,"fstype":"vfat","label":"EFI","fsused":120000000,"parttype":"c12a7328-f81f-11d2-ba4b-00a0c93ec93b","mountpoints":["/boot/efi"]},
           {"name":"sda2","path":"/dev/sda2","type":"part","size":272730423296,"fstype":"ext4","label":"root","fsused":98000000000,"parttype":null,"mountpoints":["/"]}
         ]},
        {"name":"sdb","path":"/dev/sdb","type":"disk","size":1000204886016,"model":"WD Blue","rm":false,"pttype":"gpt","fstype":null,"label":null,"fsused":null,"parttype":null,"mountpoints":[null],
         "children":[
           {"name":"sdb1","path":"/dev/sdb1","type":"part","size":1000204886016,"fstype":"ext4","label":"Backup","fsused":12000000000,"parttype":null,"mountpoints":[null]}
         ]},
        {"name":"loop0","path":"/dev/loop0","type":"loop","size":"536870912","model":null,"rm":false,"pttype":"gpt","fstype":null,"label":null,"fsused":null,"parttype":null,"mountpoints":[null]},
        {"name":"sr0","path":"/dev/sr0","type":"rom","size":0,"model":"DVD","rm":true,"pttype":null}
      ]
    }"#;

    #[test]
    fn parses_disks_and_loops_skips_rom() {
        let drives = parse_lsblk_json(SAMPLE.as_bytes()).unwrap();
        // sda + sdb + loop0, but not sr0 (rom)
        assert_eq!(drives.len(), 3);
        let sda = &drives[0];
        assert_eq!(sda.key, "/dev/sda");
        assert_eq!(sda.partition_table.as_deref(), Some("gpt"));
        assert_eq!(sda.partitions.len(), 2);
        assert_eq!(sda.partitions[0].number, 1);
        assert_eq!(sda.partitions[0].filesystem.as_deref(), Some("vfat"));
        assert_eq!(sda.partitions[1].used_bytes, Some(98000000000));
    }

    #[test]
    fn size_as_string_is_parsed() {
        let drives = parse_lsblk_json(SAMPLE.as_bytes()).unwrap();
        let loop0 = drives.iter().find(|d| d.key == "/dev/loop0").unwrap();
        assert_eq!(loop0.size_bytes, 536870912);
    }

    #[test]
    fn detects_mounted_and_system_disk() {
        let drives = parse_lsblk_json(SAMPLE.as_bytes()).unwrap();
        let sda = drives.iter().find(|d| d.key == "/dev/sda").unwrap();
        assert!(sda.is_mounted, "sda has mounted partitions");
        assert!(sda.is_system, "sda hosts / and /boot/efi");
        assert_eq!(sda.partitions[1].mountpoints, vec!["/".to_string()]);

        let sdb = drives.iter().find(|d| d.key == "/dev/sdb").unwrap();
        assert!(!sdb.is_mounted, "sdb is not mounted");
        assert!(!sdb.is_system, "sdb is not a system disk");

        let loop0 = drives.iter().find(|d| d.key == "/dev/loop0").unwrap();
        assert!(!loop0.is_mounted && !loop0.is_system);
    }
}
