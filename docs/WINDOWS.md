# Rabobi on Windows

Windows has none of the Linux tool-chain (partclone, lsblk, sfdisk), so
Rabobi uses **native Windows facilities** instead:

| Concern | Linux | Windows |
| --- | --- | --- |
| Enumerate disks | `lsblk --json` | PowerShell `Get-Disk` / `Get-Partition` / `Get-Volume` |
| Copy data | `partclone.<fs>` (used blocks only) | raw sector copy of `\\.\PhysicalDriveN` (full disk) |
| Privilege | root (`sudo`) | Administrator (elevated) |
| Verify | SHA-256 of device range | SHA-256 of device range (same code) |

## What works on Windows

- **Enumeration** — physical disks with model, size, bus type (USB → removable),
  partition style (GPT/MBR), partitions with filesystem/label/drive-letter, and
  mount/system-disk detection.
- **Clone** — full-disk raw copy from one `\\.\PhysicalDriveN` to another, with
  live progress, cancellation, and optional post-copy SHA-256 verification.
- **Backup** — writes a **full-disk raw image** to a folder: a
  `rabobi-img` JSON metadata file plus gzip-compressed, 4 GiB split
  parts (`disk.raw-img.gz.000`, `.001`, …). gzip is in-process (`flate2`), so no
  external tools are needed; "uncompressed" is also available.
- **Restore** — reads a `rabobi-img` image back onto a
  `\\.\PhysicalDriveN`, decompressing and streaming the split parts.
- **Preflight** — checks for Administrator (via `net session`) and reports
  readiness on the Home screen; real operations are refused without elevation.
- **Safety** — the same destination guardrail applies: system/boot disks are
  hard-blocked, non-empty disks require type-to-confirm.

## Differences vs Linux

- The Windows clone/backup is **not filesystem-aware**: it copies every sector,
  so it is slower and the destination (or image) must cover the **whole disk**.
  (The Linux path copies only used blocks via partclone.)
- Windows backup compression is **gzip or uncompressed** (no zstd/pigz); the UI
  hides zstd on Windows and defaults to gzip.
- The Windows image format (`rabobi-img` + raw split parts) is
  Rabobi's own, and is **not** interchangeable with the Clonezilla-format
  images the Linux path produces.

## Running

Launch elevated (right-click → *Run as administrator*, or from an elevated
terminal). Without elevation the app starts but refuses real operations.

## Building on Windows

```powershell
npm install
npm run app:build    # tauri build → .msi / .exe under src-tauri\target\release\bundle
```

Tauri produces Windows installers only when built **on** Windows (with the
WebView2 runtime and the MSVC toolchain). Cross-building from macOS/Linux is not
supported for the GUI bundle.

## A note on cross-checking from macOS

The Windows Rust code is validated from the dev machine with:

```bash
rustup target add x86_64-pc-windows-gnu
RABOBI_SKIP_TAURI_BUILD=1 cargo check --target x86_64-pc-windows-gnu --lib
```

Our modules compile cleanly for Windows this way. (A full cross-build still
fails on Tauri's own macOS-only `embed_plist` macro — that resolves when
building on a real Windows host.)
