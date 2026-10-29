//! Shared data model, mirroring the TypeScript types in src/lib/clone/types.ts.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Partition {
    pub key: String,
    pub number: u32,
    pub size_bytes: u64,
    pub filesystem: Option<String>,
    pub label: Option<String>,
    pub is_extended: bool,
    pub is_raid_member: bool,
    pub used_bytes: Option<u64>,
    /// Current mountpoint(s), if mounted (e.g. ["/", "/boot"]). Empty if not mounted.
    #[serde(default)]
    pub mountpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    pub key: String,
    pub model: String,
    pub size_bytes: u64,
    pub partition_table: Option<String>,
    pub removable: bool,
    pub partitions: Vec<Partition>,
    /// True if any partition is mounted anywhere.
    #[serde(default)]
    pub is_mounted: bool,
    /// True if this disk hosts a critical system mountpoint (/, /boot, /usr,
    /// swap, etc.) — writing to it would destroy the running OS.
    #[serde(default)]
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionMappingRow {
    pub source_key: String,
    pub enabled: bool,
    pub source_description: String,
    pub dest_key: String,
    pub dest_description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRequest {
    pub source_drive_key: String,
    pub destination_drive_key: String,
    pub overwrite_partition_table: bool,
    pub rescue: bool,
    pub post_action: String,
    pub mapping: Vec<PartitionMappingRow>,
    /// Device the user typed to confirm overwriting a non-blank destination.
    #[serde(default)]
    pub confirm_device: Option<String>,
    /// Verify each copied partition afterwards by hashing source vs dest. (5.4)
    #[serde(default)]
    pub verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionProgress {
    pub source_key: String,
    pub dest_key: String,
    pub fraction: f64,
    pub status_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneProgressEvent {
    pub status: String,
    pub overall_fraction: f64,
    pub current: Option<PartitionProgress>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_line: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneSummary {
    pub status: String,
    pub title: String,
    pub lines: Vec<String>,
    pub duration_seconds: u64,
    /// Non-fatal problems encountered (e.g. recoverable read errors, a skipped
    /// optional step). The operation still completed. (Milestone 5.7)
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Path to the per-operation log file, if one was written. (Milestone 5.5)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
}

// ---- Backup ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRequest {
    pub source_drive_key: String,
    /// short partition nodes to back up, e.g. ["sda1","sda2"]; empty = all
    #[serde(default)]
    pub partitions: Vec<String>,
    pub dest_dir: String,
    /// "gzip" | "zstd" | "uncompressed"
    pub compression: String,
    #[serde(default)]
    pub compression_level: Option<u32>,
    #[serde(default)]
    pub rescue: bool,
    #[serde(default)]
    pub notes: String,
    /// Hash the source while imaging and record the digest in the image
    /// metadata, so restore can self-verify. (5.4 / 6.6)
    #[serde(default)]
    pub verify: bool,
}

// ---- Restore ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreRequest {
    /// path to the Clonezilla image directory
    pub image_dir: String,
    pub destination_drive_key: String,
    pub overwrite_partition_table: bool,
    #[serde(default)]
    pub rescue: bool,
    /// image-partition -> dest-partition mapping; empty = derive 1:1
    #[serde(default)]
    pub mapping: Vec<PartitionMappingRow>,
    /// Device the user typed to confirm overwriting a non-blank destination.
    #[serde(default)]
    pub confirm_device: Option<String>,
    /// Verify each restored partition afterwards. (5.4)
    #[serde(default)]
    pub verify: bool,
}

/// A scanned image directory's summary, for the restore picker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub path: String,
    pub name: String,
    pub disk: String,
    pub last_modified: String,
    pub size_bytes: u64,
    pub notes: String,
    pub partitions: Vec<ImagePartitionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePartitionInfo {
    pub key: String,
    pub filesystem: Option<String>,
    pub compression: String,
    pub size_bytes: u64,
}

/// A directory entry, for the folder browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}


#[derive(Debug, thiserror::Error)]
pub enum CloneError {
    #[error("this platform does not support real disk operations (Linux with partclone/sfdisk required)")]
    UnsupportedPlatform,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    #[error("command `{0}` failed: {1}")]
    Command(String, String),
    #[error("{0}")]
    Other(String),
}

impl Serialize for CloneError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
