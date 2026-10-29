//! Post-operation verification (Milestone 5.4).
//!
//! After a clone or restore, optionally read back the destination and compare
//! it against the source by streaming SHA-256 over a bounded byte range. This
//! is a best-effort integrity check: for filesystem-aware copies we can only
//! compare the number of bytes that were actually written (the used size), so
//! callers pass the byte length to hash.
//!
//! Cross-platform: operates on raw device paths, which exist on both Linux
//! (`/dev/sdXN`) and Windows (`\\.\PhysicalDriveN`).

use crate::clone::CancelToken;
use crate::model::CloneError;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;

const CHUNK: usize = 4 * 1024 * 1024; // 4 MiB

/// Hash up to `len` bytes of `path`. If `len` is 0, hashes the whole file.
pub fn hash_device(path: &str, len: u64, cancel: &CancelToken) -> Result<String, CloneError> {
    let mut file =
        File::open(path).map_err(|e| CloneError::Other(format!("verify: open {path}: {e}")))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut remaining = if len == 0 { u64::MAX } else { len };

    while remaining > 0 {
        if cancel.is_cancelled() {
            return Err(CloneError::Other("verification cancelled".into()));
        }
        let want = remaining.min(CHUNK as u64) as usize;
        let n = file
            .read(&mut buf[..want])
            .map_err(|e| CloneError::Other(format!("verify: read {path}: {e}")))?;
        if n == 0 {
            break; // EOF
        }
        hasher.update(&buf[..n]);
        remaining = remaining.saturating_sub(n as u64);
    }
    Ok(hex(&hasher.finalize()))
}

/// Compare `len` bytes of two devices. Returns Ok(true) when identical.
#[cfg_attr(not(any(target_os = "linux", target_os = "windows")), allow(dead_code))]
pub fn compare_devices(
    source: &str,
    dest: &str,
    len: u64,
    cancel: &CancelToken,
) -> Result<bool, CloneError> {
    let a = hash_device(source, len, cancel)?;
    let b = hash_device(dest, len, cancel)?;
    Ok(a == b)
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn hashes_a_file_and_matches_itself() {
        let dir = std::env::temp_dir();
        let p = dir.join("rc-verify-test.bin");
        {
            let mut f = File::create(&p).unwrap();
            f.write_all(b"the quick brown fox jumps over the lazy dog").unwrap();
        }
        let token = CancelToken::default();
        let ps = p.to_str().unwrap();
        let h1 = hash_device(ps, 0, &token).unwrap();
        let h2 = hash_device(ps, 0, &token).unwrap();
        assert_eq!(h1, h2);
        // Known SHA-256 of that text.
        assert_eq!(h1, "05c6e08f1d9fdafa03147fcb8f82f124c76d2f70e3d989dc8aadb5e7d7450bec");
        assert!(compare_devices(ps, ps, 0, &token).unwrap());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn bounded_length_hashes_prefix_only() {
        let dir = std::env::temp_dir();
        let p = dir.join("rc-verify-test2.bin");
        {
            let mut f = File::create(&p).unwrap();
            f.write_all(b"AAAA....tail that should be ignored").unwrap();
        }
        let token = CancelToken::default();
        let ps = p.to_str().unwrap();
        let full = hash_device(ps, 0, &token).unwrap();
        let prefix = hash_device(ps, 4, &token).unwrap();
        assert_ne!(full, prefix);
        let _ = std::fs::remove_file(&p);
    }
}
