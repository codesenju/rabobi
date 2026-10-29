pub mod backup;
pub mod clone;
pub mod drives;
pub mod logger;
#[cfg(any(target_os = "macos", test))]
pub mod macos;
pub mod model;
pub mod pipeline;
pub mod preflight;
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub mod rawio;
pub mod restore;
pub mod safety;
pub mod verify;
#[cfg(any(target_os = "windows", target_os = "macos", test))]
pub mod windows;

use clone::CancelToken;
use model::{
    BackupRequest, CloneRequest, CloneSummary, DirEntryInfo, Drive, ImageInfo, RestoreRequest,
};
use preflight::Preflight;
use safety::DestinationSafety;
use std::sync::Mutex;
use tauri::{Emitter, State};
#[cfg(debug_assertions)]
use tauri::Manager;

/// App-wide state: the cancel token for the in-flight operation (if any).
#[derive(Default)]
struct AppState {
    cancel: Mutex<Option<CancelToken>>,
}

const PROGRESS_EVENT: &str = "clone://progress";

#[tauri::command]
fn list_drives() -> Result<Vec<Drive>, model::CloneError> {
    drives::list_drives()
}

#[tauri::command]
fn preflight() -> Result<Preflight, model::CloneError> {
    preflight::preflight()
}

#[tauri::command]
fn scan_images(dir: String) -> Result<Vec<ImageInfo>, model::CloneError> {
    restore::scan_images(&dir)
}

/// Evaluate whether a destination disk is safe to overwrite (Milestone 5.3).
/// The UI calls this before showing the confirm step so it can require the user
/// to type the device name when there are warnings.
#[tauri::command]
fn check_destination(
    destination: String,
    source: Option<String>,
) -> Result<DestinationSafety, model::CloneError> {
    let drives = drives::list_drives()?;
    Ok(safety::evaluate_destination(&drives, &destination, source.as_deref()))
}

/// List subdirectories of a path, for the destination-folder browser.
#[tauri::command]
fn list_dirs(path: String) -> Result<Vec<DirEntryInfo>, model::CloneError> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&path)
        .map_err(|e| model::CloneError::Other(format!("read_dir {path}: {e}")))?;
    for entry in entries.flatten() {
        let p = entry.path();
        let is_dir = p.is_dir();
        if !is_dir {
            continue;
        }
        out.push(DirEntryInfo {
            name: entry.file_name().to_string_lossy().to_string(),
            path: p.to_string_lossy().to_string(),
            is_dir,
        });
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

#[tauri::command]
async fn start_clone(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: CloneRequest,
) -> Result<CloneSummary, model::CloneError> {
    run_op(app, state, move |token, emit| clone::run_clone(&request, token, emit)).await
}

#[tauri::command]
async fn start_backup(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: BackupRequest,
) -> Result<CloneSummary, model::CloneError> {
    run_op(app, state, move |token, emit| backup::run_backup(&request, token, emit)).await
}

#[tauri::command]
async fn start_restore(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: RestoreRequest,
) -> Result<CloneSummary, model::CloneError> {
    run_op(app, state, move |token, emit| restore::run_restore(&request, token, emit)).await
}

#[tauri::command]
fn cancel_operation(state: State<'_, AppState>) {
    if let Some(token) = state.cancel.lock().unwrap().as_ref() {
        token.cancel();
    }
}

/// Shared runner: installs a cancel token, runs the blocking engine on a
/// worker thread, forwards progress events to the webview.
async fn run_op<Feng>(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    engine: Feng,
) -> Result<CloneSummary, model::CloneError>
where
    Feng: FnOnce(
            &CancelToken,
            &mut dyn FnMut(model::CloneProgressEvent),
        ) -> Result<CloneSummary, model::CloneError>
        + Send
        + 'static,
{
    let token = CancelToken::default();
    *state.cancel.lock().unwrap() = Some(token.clone());
    let app_handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut emit = |event: model::CloneProgressEvent| {
            let _ = app_handle.emit(PROGRESS_EVENT, event);
        };
        engine(&token, &mut emit)
    })
    .await
    .map_err(|e| model::CloneError::Other(format!("join error: {e}")))?;
    *state.cancel.lock().unwrap() = None;
    result
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_drives,
            preflight,
            scan_images,
            check_destination,
            list_dirs,
            start_clone,
            start_backup,
            start_restore,
            cancel_operation
        ])
        .setup(|_app| {
            #[cfg(debug_assertions)]
            {
                let _ = _app.get_webview_window("main");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Rabobi");
}
