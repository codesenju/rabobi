//! Preflight checks (Milestone 5.1 / 5.2): is the app able to perform real
//! disk operations right now? Reports platform, root/privilege status, and
//! which required external tools are present.

use crate::model::CloneError;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub name: String,
    pub present: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preflight {
    /// OS family: "linux" | "windows" | "macos" | "other"
    pub os: String,
    /// true on platforms where real operations are possible (Linux, Windows)
    pub supported_platform: bool,
    /// true if running with the privileges needed for raw disk access
    /// (root on Linux, Administrator on Windows)
    pub is_root: bool,
    /// required tools and whether each is on PATH
    pub tools: Vec<ToolStatus>,
    /// true when every required tool is present
    pub all_tools_present: bool,
    /// overall: real operations are safe to run
    pub ready: bool,
    /// human-readable blockers, if any
    pub blockers: Vec<String>,
}

/// Required external tools per platform. Windows uses native file APIs +
/// PowerShell (always present), so it has no third-party requirements.
#[cfg(target_os = "linux")]
const REQUIRED_TOOLS: &[&str] = &[
    "lsblk", "sfdisk", "sgdisk", "partprobe", "partclone.dd", "pigz", "zstd", "split", "dd", "sync",
];
#[cfg(not(target_os = "linux"))]
const REQUIRED_TOOLS: &[&str] = &[];

pub fn preflight() -> Result<Preflight, CloneError> {
    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "other"
    }
    .to_string();
    let supported_platform = cfg!(target_os = "linux") || cfg!(target_os = "windows") || cfg!(target_os = "macos");
    let is_root = is_elevated();
    let priv_name = if cfg!(target_os = "windows") { "Administrator" } else { "root" };

    let tools: Vec<ToolStatus> = REQUIRED_TOOLS
        .iter()
        .map(|t| ToolStatus {
            name: (*t).to_string(),
            present: which(t),
        })
        .collect();
    let all_tools_present = tools.iter().all(|t| t.present);

    let mut blockers = Vec::new();
    if !supported_platform {
        blockers.push("This platform cannot perform real disk operations (Linux, Windows, or macOS required).".into());
    }
    if supported_platform && !is_root {
        blockers.push(format!(
            "Not running as {priv_name} — real disk operations require elevated privileges."
        ));
    }
    if supported_platform {
        let missing: Vec<&str> = tools.iter().filter(|t| !t.present).map(|t| t.name.as_str()).collect();
        if !missing.is_empty() {
            blockers.push(format!("Missing required tools: {}", missing.join(", ")));
        }
    }

    let ready = supported_platform && is_root && all_tools_present;

    Ok(Preflight {
        os,
        supported_platform,
        is_root,
        tools,
        all_tools_present,
        ready,
        blockers,
    })
}

/// Are we running with the privileges required for raw disk access?
#[cfg(unix)]
fn is_elevated() -> bool {
    current_euid() == 0
}

#[cfg(windows)]
fn is_elevated() -> bool {
    // `net session` succeeds only for an elevated (Administrator) context.
    use std::process::Command;
    Command::new("net")
        .arg("session")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(any(unix, windows)))]
fn is_elevated() -> bool {
    false
}

#[cfg(unix)]
fn current_euid() -> u32 {
    // Avoid an extra dependency: read from `id -u`. Safe, cheap, Unix-only.
    use std::process::Command;
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(1000)
}

fn which(tool: &str) -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
        Command::new("sh")
            .args(["-c", &format!("command -v {tool}")])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        use std::process::Command;
        Command::new("where")
            .arg(tool)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = tool;
        false
    }
}

/// Guard used by the operation commands: returns an error if not ready.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn require_ready() -> Result<(), CloneError> {
    let pf = preflight()?;
    if pf.ready {
        Ok(())
    } else {
        Err(CloneError::Other(format!(
            "cannot start operation: {}",
            pf.blockers.join("; ")
        )))
    }
}
