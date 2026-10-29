//! Shared compression/decompression helpers, mirroring Rescuezilla's
//! utility.py get_compression_cmd_list / get_decompression_command_list /
//! get_compression_suffix.

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn compression_suffix(format: &str) -> &'static str {
    match format {
        "gzip" => "gz",
        "zstd" => "zst",
        "bzip2" => "bz2",
        _ => "uncomp",
    }
}

/// The compressor command that reads stdin and writes compressed stdout.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn compression_cmd(format: &str, level: Option<u32>) -> Vec<String> {
    let lvl = level.unwrap_or(match format {
        "gzip" => 6,
        "zstd" => 15,
        "bzip2" => 9,
        _ => 0,
    });
    match format {
        "gzip" => vec![
            "pigz".into(),
            "--stdout".into(),
            format!("-{lvl}"),
        ],
        "zstd" => vec![
            "zstd".into(),
            "--stdout".into(),
            format!("-{lvl}"),
            "--threads=0".into(),
        ],
        "bzip2" => vec!["pbzip2".into(), "--stdout".into(), format!("-{lvl}")],
        _ => vec!["cat".into(), "-".into()],
    }
}

/// The decompressor command that reads stdin and writes plaintext stdout.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn decompression_cmd(format: &str) -> Vec<String> {
    match format {
        "gzip" | "gz" => vec!["pigz".into(), "-d".into(), "-c".into()],
        "zstd" | "zst" => vec!["zstd".into(), "-d".into(), "-c".into()],
        "bzip2" | "bz2" => vec!["pbzip2".into(), "-d".into(), "-c".into()],
        "xz" => vec!["unxz".into(), "-c".into()],
        "lz4" => vec!["unlz4".into(), "-d".into(), "-".into()],
        _ => vec!["cat".into(), "-".into()],
    }
}

/// Map a filesystem to the specialised partclone binary.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn partclone_binary(fs: &str) -> String {
    let name = match fs {
        "ntfs" => "ntfs",
        "ext2" | "ext3" | "ext4" => "extfs",
        "fat" | "fat12" | "fat16" | "fat32" | "vfat" => "fat",
        "exfat" => "exfat",
        "xfs" => "xfs",
        "btrfs" => "btrfs",
        "hfsplus" => "hfsp",
        "" | "raw" => "dd",
        _ => "dd",
    };
    format!("partclone.{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes() {
        assert_eq!(compression_suffix("gzip"), "gz");
        assert_eq!(compression_suffix("zstd"), "zst");
        assert_eq!(compression_suffix("uncompressed"), "uncomp");
    }

    #[test]
    fn partclone_mapping() {
        assert_eq!(partclone_binary("ext4"), "partclone.extfs");
        assert_eq!(partclone_binary("ntfs"), "partclone.ntfs");
        assert_eq!(partclone_binary(""), "partclone.dd");
    }

    #[test]
    fn compressor_roundtrip_shape() {
        assert_eq!(compression_cmd("gzip", Some(1))[0], "pigz");
        assert_eq!(decompression_cmd("gz")[0], "pigz");
    }
}
