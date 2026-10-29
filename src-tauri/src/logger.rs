//! Per-operation structured logging (Milestone 5.5).
//!
//! Each clone/backup/restore run opens a timestamped log file and records a
//! header, every command the engine runs, progress milestones, and the final
//! summary. This mirrors the role of Rescuezilla's `clonezilla-img` log and
//! gives the user an audit trail of exactly what was executed.
//!
//! Log location (first that is writable):
//!   $RABOBI_LOG_DIR, else /var/log/rabobi, else $TMPDIR/rabobi
//! Filename: rabobi-<op>-<YYYYMMDD-HHMMSS>.log

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct OperationLog {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl OperationLog {
    /// Open a new log for the given operation ("clone"/"backup"/"restore").
    pub fn open(op: &str) -> OperationLog {
        let dir = choose_log_dir();
        let name = format!("rabobi-{op}-{}.log", timestamp_compact());
        let path = dir.join(name);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        let log = OperationLog {
            path: path.clone(),
            file: Mutex::new(file),
        };
        log.line(&format!("=== Rabobi {op} log ==="));
        log.line(&format!("started: {}", timestamp_iso()));
        log.line(&format!("logfile: {}", path.display()));
        log
    }

    /// A disabled log that writes nothing (used where a log is not wanted).
    pub fn disabled() -> OperationLog {
        OperationLog {
            path: PathBuf::new(),
            file: Mutex::new(None),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write one timestamped line.
    pub fn line(&self, text: &str) {
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = writeln!(f, "[{}] {}", timestamp_iso(), text);
                let _ = f.flush();
            }
        }
    }

    /// Record a command the engine is about to run.
    pub fn command(&self, cmd: &str) {
        self.line(&format!("$ {cmd}"));
    }

    /// Record the final summary block.
    pub fn summary(&self, status: &str, lines: &[String], duration_secs: u64) {
        self.line(&format!("--- summary: {status} ({duration_secs}s) ---"));
        for l in lines {
            self.line(&format!("  {l}"));
        }
        self.line("=== end ===");
    }
}

fn choose_log_dir() -> PathBuf {
    let candidates = [
        std::env::var("RABOBI_LOG_DIR").ok().map(PathBuf::from),
        Some(PathBuf::from("/var/log/rabobi")),
        Some(std::env::temp_dir().join("rabobi")),
    ];
    for cand in candidates.into_iter().flatten() {
        if fs::create_dir_all(&cand).is_ok() && is_writable(&cand) {
            return cand;
        }
    }
    std::env::temp_dir()
}

fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".rescueclone-write-probe");
    match File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn timestamp_iso() -> String {
    datetime(false)
}

fn timestamp_compact() -> String {
    datetime(true)
}

/// Format the current UTC time without pulling in chrono.
fn datetime(compact: bool) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, m, d) = civil_from_days(days as i64);
    if compact {
        format!("{y:04}{m:02}{d:02}-{h:02}{mi:02}{s:02}")
    } else {
        format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}Z")
    }
}

// Howard Hinnant's days->civil algorithm (also used in restore.rs).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_back() {
        std::env::set_var("RABOBI_LOG_DIR", std::env::temp_dir().join("rc-log-test"));
        let log = OperationLog::open("clone");
        log.command("partclone.dd --source /dev/loop0 --overwrite /dev/loop1");
        log.summary("completed", &["loop0 -> loop1: ok".into()], 12);
        let contents = std::fs::read_to_string(log.path()).unwrap();
        assert!(contents.contains("Rabobi clone log"));
        assert!(contents.contains("$ partclone.dd"));
        assert!(contents.contains("summary: completed"));
        std::env::remove_var("RABOBI_LOG_DIR");
    }

    #[test]
    fn compact_timestamp_shape() {
        let t = timestamp_compact();
        // YYYYMMDD-HHMMSS
        assert_eq!(t.len(), 15);
        assert_eq!(&t[8..9], "-");
    }
}
