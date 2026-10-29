fn main() {
    // Escape hatch for cross-target `cargo check` (e.g. validating the Windows
    // build from a non-Windows host) where the full Tauri bundle toolchain is
    // unavailable. Real app builds leave this unset.
    if std::env::var_os("RABOBI_SKIP_TAURI_BUILD").is_some() {
        println!("cargo:rustc-check-cfg=cfg(desktop)");
        println!("cargo:rustc-check-cfg=cfg(mobile)");
        return;
    }
    tauri_build::build()
}
