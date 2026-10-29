# RescueClone — Product Requirements & Delivery Tracker

A cross-platform desktop disk tool reverse-engineered from
[Rescuezilla](https://github.com/rescuezilla/rescuezilla), built with
[Tauri](https://tauri.app) (Rust) + React/TypeScript and [beUI](https://beui.dev)
components.

## Goal

Ship a tool we can actually use to **clone, back up, and restore drives** on a
Linux host — Clonezilla-compatible, with a modern GUI. Real disk operations run
on Linux with root; macOS/Windows run in safe simulation mode for development.

## Status legend

| Mark | Meaning |
| --- | --- |
| ✅ | Done & verified |
| 🚧 | In progress |
| ⏳ | Not started |
| 🧪 | Implemented, needs real-hardware verification |
| ⛔ | Blocked |

---

## Milestone 1 — Clone feature (shipped)

| # | Step | Status | Notes |
| --- | --- | --- | --- |
| 1.1 | Reverse-engineer Rescuezilla clone path | ✅ | handler/clone_manager/restore_manager/metadata_only_image/partitions_to_restore |
| 1.2 | Scaffold Tauri + React + Vite + Tailwind + beUI | ✅ | `npm run build` + `cargo check` clean |
| 1.3 | beUI components (button, checkbox, select, modal, hooks) | ✅ | copied from beUI registry (MIT) |
| 1.4 | Clone data model + mapping logic (TS) | ✅ | `src/lib/clone/` |
| 1.5 | 7-page clone wizard UI | ✅ | verified end-to-end with Playwright |
| 1.6 | Rust backend: `list_drives` via lsblk | ✅ | Linux only; else UnsupportedPlatform |
| 1.7 | Rust backend: `start_clone`/`cancel_clone` + partclone --dev-to-dev | ✅ | compiles; not yet run on real hardware |
| 1.8 | Browser simulation mode | ✅ | fake drives + simulated progress |

---

## Milestone 2 — Real Linux test harness

Verify the native backend against real block devices without risking a real disk.

| # | Step | Status | Notes |
| --- | --- | --- | --- |
| 2.1 | Document Linux prerequisites (partclone, sfdisk, lsblk, gdisk, pigz/zstd) | ✅ | see `docs/LINUX_TESTING.md` |
| 2.2 | Loopback-device test script (create 2 fake disks from image files) | ✅ | `scripts/make-test-disks.sh` |
| 2.3 | `cargo test` unit tests for parsers (lsblk JSON, partclone progress) | ✅ | `src-tauri/src/*` `#[cfg(test)]` |
| 2.4 | Integration test: list_drives against loopback devices | ✅ | `scripts/test-list-drives.sh` (gated on Linux+root) |
| 2.5 | Integration test: clone loop0 → loop1, verify with cmp/sha | ✅ | `scripts/test-clone.sh` |
| 2.6 | Run harness on a real Linux host | 🧪 | needs a Linux machine; scripts ready |

---

## Milestone 3 — Backup feature

Create a Clonezilla-compatible image directory from a source disk.

| # | Step | Status | Notes |
| --- | --- | --- | --- |
| 3.1 | Backup data model + types (TS) | ✅ | `src/lib/backup/types.ts` |
| 3.2 | Backup wizard UI (source → partitions → destination → name/notes → compression → confirm → progress → summary) | ✅ | beUI components |
| 3.3 | Rust `list_dirs`/path browse command for destination folder | ✅ | `list_directory` command |
| 3.4 | Rust `start_backup`: partclone.<fs> \| compressor \| split pipeline | ✅ | gzip/zstd/uncompressed; writes image dir |
| 3.5 | Write Clonezilla image dir layout (parts, disk, -pt.sf, -mbr, .info, images) | ✅ | core subset; GPT backup files included |
| 3.6 | Byte-weighted backup progress events | ✅ | reuses progress model |
| 3.7 | Backup simulation mode | ✅ | |
| 3.8 | Mode switcher (Clone / Backup / Restore) in UI shell | ✅ | home screen |
| 3.9 | Verify backup on real hardware | 🧪 | via Milestone 2 harness |

---

## Milestone 4 — Restore feature

Restore a scanned image directory onto a destination disk.

| # | Step | Step | Status | Notes |
| --- | --- | --- | --- | --- |
| 4.1 | Restore data model + types (TS) | | ✅ | `src/lib/restore/types.ts` |
| 4.2 | Rust `scan_images`: detect Clonezilla image dirs (identified by `parts`) | | ✅ | parses metadata for the list |
| 4.3 | Restore wizard UI (source folder → image → destination → mapping → confirm → progress → summary) | | ✅ | reuses mapping table |
| 4.4 | Rust `start_restore`: sfdisk table write + cat \| decompress \| partclone.restore --overwrite | | ✅ | |
| 4.5 | Byte-weighted restore progress events | | ✅ | |
| 4.6 | Restore simulation mode | | ✅ | |
| 4.7 | Verify restore on real hardware | | 🧪 | via Milestone 2 harness |

---

## Milestone 5 — Production readiness

Steps to make this safe and pleasant to actually copy drives with.

| # | Step | Status | Notes |
| --- | --- | --- | --- |
| 5.1 | Root/privilege check + clear error when not root | ✅ | `preflight.rs` require_ready(); blocks real ops if euid != 0 |
| 5.2 | Preflight tool check (partclone.*, sfdisk, sgdisk, pigz, zstd present) | ✅ | `preflight` command; shown on Home screen |
| 5.3 | Destination safety: refuse mounted/system/source disks; confirm by typing device | ✅ | `safety.rs` + `check_destination` cmd + `DestinationGuard` type-to-confirm UI; hard-blocks system/source, enforced in all engines |
| 5.4 | Post-op verification (sha/cmp of restored partitions where feasible) | ✅ | `verify.rs` SHA-256; opt-in toggle in clone/restore **and backup** (backup records a source digest in the image; restore self-checks against it) |
| 5.5 | Structured logging to a log file per operation | ✅ | `logger.rs`; per-op timestamped log with every command + summary |
| 5.6 | Cancellation hardening (SIGTERM whole pipeline, cleanup partials) | ✅ | kills whole pipe; backup removes partial image files; cancel surfaced as warning |
| 5.7 | Error surfacing: non-fatal partclone errors → summary warnings | ✅ | rescue-mode failures downgraded to warnings; `CloneSummary.warnings` shown in UI |
| 5.8 | AppImage/.deb packaging for Linux | 🧪 | bundle config set (deb/appimage/rpm + runtime depends); `docs/PACKAGING.md`; needs a Linux host to produce artifacts |
| 5.9 | Bootable-USB / live-environment story (optional) | ⏳ | run from a live distro |
| 5.10 | Real icons + app metadata (currently placeholder) | ✅ | `icons/rabobi.svg` source → PNGs + `.icns` + `.ico`; three-arrow "pointing" mark; used in bundle, favicon, and Home header |
| 5.11 | End-to-end smoke test on real hardware with a throwaway disk | 🧪 | gate before first real use |
| 5.12 | User docs: how to clone/backup/restore safely | ✅ | README + docs/LINUX_TESTING.md + docs/PACKAGING.md |

---

## Milestone 6 — Windows support

Windows has no partclone/lsblk, so RescueClone uses native facilities: PowerShell
`Get-Disk` enumeration and raw `\\.\PhysicalDriveN` sector copy.

| # | Step | Status | Notes |
| --- | --- | --- | --- |
| 6.1 | Platform-dispatch `list_drives` | ✅ | `drives.rs` → Linux lsblk / Windows PowerShell |
| 6.2 | Windows disk enumeration (Get-Disk/Get-Partition/Get-Volume → model) | ✅ | `windows.rs::parse_windows_json`; mount/system detection |
| 6.3 | Windows clone engine (raw full-disk copy + progress + cancel) | ✅ | `windows.rs::run_clone_windows` / `raw_copy` |
| 6.4 | Preflight: Administrator + platform check on Windows | ✅ | `preflight.rs` `is_elevated()` via `net session`; `where` for tools |
| 6.5 | Post-op verification on Windows | ✅ | `verify.rs` is cross-platform (raw device hashing) |
| 6.6 | Windows backup/restore (image to/from folder) | ✅ | `windows.rs` raw full-disk image (`flate2` gzip) + split parts + scanner; UI hides zstd / notes full-disk on Windows |
| 6.7 | Cross-compile validation of Windows code | ✅ | `cargo check --target x86_64-pc-windows-gnu` clean for our modules (tauri's own macOS-plist macro blocks full cross-build from macOS; builds on a Windows host) |
| 6.8 | Windows `.msi`/`.exe` packaging | 🧪 | `tauri build` on a Windows host; config targets "all" by default there |
| 6.9 | Real-hardware Windows smoke test | 🧪 | gate before first real use on Windows |

---

## How to use this tracker

Update the Status column as work lands. Keep one source of truth here; link PRs
or commits in Notes. "🧪 needs real-hardware verification" items are the gate
before using RescueClone on a drive that matters.
