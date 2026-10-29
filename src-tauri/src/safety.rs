//! Destination safety guardrails (Milestone 5.3).
//!
//! Before any destructive write (clone destination, restore destination) we
//! verify the target device is safe to overwrite:
//!   - it is not the source disk,
//!   - it is not currently mounted,
//!   - it does not host the running OS (/, /boot, swap, …).
//!
//! The backend *enforces* these: a blocked destination can only proceed when
//! the caller passes an explicit `confirm_device` matching the device node,
//! which the UI collects by making the user type the device name. This mirrors
//! the "type the name to confirm" pattern used by destructive cloud consoles.

use crate::model::{CloneError, Drive};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationSafety {
    pub device: String,
    /// Hard blockers that cannot be overridden (e.g. same as source, system disk).
    pub hard_blocks: Vec<String>,
    /// Soft warnings that require explicit type-to-confirm (e.g. mounted, has data).
    pub warnings: Vec<String>,
    /// True when the destination can proceed with no confirmation.
    pub safe: bool,
    /// True when the destination may proceed only with type-to-confirm.
    pub needs_confirmation: bool,
}

/// Evaluate a destination. `source` is the device being read (clone) or the
/// image's origin disk node (restore), used to reject self-overwrite.
pub fn evaluate_destination(
    drives: &[Drive],
    destination: &str,
    source: Option<&str>,
) -> DestinationSafety {
    let mut hard_blocks = Vec::new();
    let mut warnings = Vec::new();

    let drive = drives.iter().find(|d| d.key == destination);

    match drive {
        None => {
            hard_blocks.push(format!("Destination {destination} is not a recognised disk."));
        }
        Some(d) => {
            if let Some(src) = source {
                if src == destination {
                    hard_blocks.push("Destination is the same as the source.".into());
                }
            }
            if d.is_system {
                hard_blocks
                    .push("Destination hosts the running operating system (/, /boot, or swap).".into());
            }
            if d.is_mounted && !d.is_system {
                warnings.push(
                    "Destination has mounted filesystems — they will be unmounted and erased.".into(),
                );
            }
            if !d.partitions.is_empty() {
                warnings.push(format!(
                    "Destination already contains {} partition(s); all data will be erased.",
                    d.partitions.len()
                ));
            }
            if d.removable {
                // Informational: removable is the *expected* safe case, no warning.
            }
        }
    }

    let safe = hard_blocks.is_empty() && warnings.is_empty();
    let needs_confirmation = hard_blocks.is_empty() && !warnings.is_empty();

    DestinationSafety {
        device: destination.to_string(),
        hard_blocks,
        warnings,
        safe,
        needs_confirmation,
    }
}

/// Enforce safety for a real operation. Returns Ok only when the destination is
/// safe, or needs confirmation and `confirm_device` exactly matches it. Any
/// hard block always fails.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn enforce_destination(
    drives: &[Drive],
    destination: &str,
    source: Option<&str>,
    confirm_device: Option<&str>,
) -> Result<(), CloneError> {
    let s = evaluate_destination(drives, destination, source);
    if !s.hard_blocks.is_empty() {
        return Err(CloneError::Other(format!(
            "destination refused: {}",
            s.hard_blocks.join("; ")
        )));
    }
    if s.needs_confirmation {
        let confirmed = confirm_device
            .map(|c| c.trim() == destination || c.trim() == destination.trim_start_matches("/dev/"))
            .unwrap_or(false);
        if !confirmed {
            return Err(CloneError::Other(format!(
                "destination {destination} requires confirmation: {}",
                s.warnings.join("; ")
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Drive, Partition};

    fn drive(key: &str, mounted: bool, system: bool, parts: usize) -> Drive {
        Drive {
            key: key.into(),
            model: "Test".into(),
            size_bytes: 1_000_000_000,
            partition_table: Some("gpt".into()),
            removable: false,
            is_mounted: mounted,
            is_system: system,
            partitions: (0..parts)
                .map(|i| Partition {
                    key: format!("{key}{}", i + 1),
                    number: (i + 1) as u32,
                    size_bytes: 100,
                    filesystem: Some("ext4".into()),
                    label: None,
                    is_extended: false,
                    is_raid_member: false,
                    used_bytes: Some(10),
                    mountpoints: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn blocks_system_disk() {
        let drives = vec![drive("/dev/sda", true, true, 2)];
        let s = evaluate_destination(&drives, "/dev/sda", None);
        assert!(!s.hard_blocks.is_empty());
        assert!(!s.safe && !s.needs_confirmation);
        assert!(enforce_destination(&drives, "/dev/sda", None, Some("/dev/sda")).is_err());
    }

    #[test]
    fn blocks_same_as_source() {
        let drives = vec![drive("/dev/sdb", false, false, 0)];
        let s = evaluate_destination(&drives, "/dev/sdb", Some("/dev/sdb"));
        assert!(s.hard_blocks.iter().any(|b| b.contains("same as the source")));
    }

    #[test]
    fn warns_on_existing_data_and_requires_confirm() {
        let drives = vec![drive("/dev/sdb", false, false, 1)];
        let s = evaluate_destination(&drives, "/dev/sdb", Some("/dev/sda"));
        assert!(s.needs_confirmation);
        // wrong / missing confirmation fails
        assert!(enforce_destination(&drives, "/dev/sdb", Some("/dev/sda"), None).is_err());
        assert!(enforce_destination(&drives, "/dev/sdb", Some("/dev/sda"), Some("/dev/sdX")).is_err());
        // correct confirmation (short or full) passes
        assert!(enforce_destination(&drives, "/dev/sdb", Some("/dev/sda"), Some("sdb")).is_ok());
        assert!(enforce_destination(&drives, "/dev/sdb", Some("/dev/sda"), Some("/dev/sdb")).is_ok());
    }

    #[test]
    fn blank_disk_is_safe() {
        let drives = vec![drive("/dev/sdb", false, false, 0)];
        let s = evaluate_destination(&drives, "/dev/sdb", Some("/dev/sda"));
        assert!(s.safe);
        assert!(enforce_destination(&drives, "/dev/sdb", Some("/dev/sda"), None).is_ok());
    }
}
