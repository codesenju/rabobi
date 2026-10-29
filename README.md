# Rabobi

A cross-platform **desktop disk tool** — **clone, back up, and restore** drives —
reverse-engineered from [Rescuezilla](https://github.com/rescuezilla/rescuezilla)
(the GTK3/Python Clonezilla front-end), with a UI built from
[beUI](https://beui.dev) animated React components.

> **Rabobi** is the Xhosa-language dubbed version of the classic 1980s Spider-Man
> cartoon. The name is a nod to the "three Spider-Men pointing at each other"
> meme — a perfect mascot for a tool whose whole job is making disks into
> identical copies of one another.

It reproduces Rescuezilla's three core workflows as a modern
[Tauri](https://tauri.app) desktop app:

- **Clone** — copy one disk directly onto another (device-to-device, no image file)
- **Backup** — save a disk to a compressed, Clonezilla-compatible image folder
- **Restore** — write a previously-created image back onto a disk

Real disk operations run on **Linux** (as root), **Windows** (as Administrator),
and **macOS** (as root); other environments and plain `vite dev` run a full
**simulation mode** so the entire UX is usable for development and demos.

See [PRD.md](PRD.md) for the delivery tracker and [docs/LINUX_TESTING.md](docs/LINUX_TESTING.md)
for the safe loopback test harness.

## What was reverse-engineered

The original workflows live in Rescuezilla's Python sources:

| Rescuezilla source | What it does | Reimplemented here as |
| --- | --- | --- |
| `handler.py` (`next_tab` state machine) | clone/backup/restore wizard flows | `src/wizards/*.tsx` |
| `drive_query.py` + `combined_drive_state.py` | enumerate disks/partitions via lsblk | `src-tauri/src/drives.rs` (`lsblk --json`) |
| `partitions_to_restore.py` | source→destination partition mapping | `src/lib/clone/mapping.ts` + `PartitionMappingTable.tsx` |
| `clone_manager.py` + `restore_manager.py` (is_cloning) | partclone `--dev-to-dev` copy | `src-tauri/src/clone.rs` |
| `backup_manager.py` | Clonezilla image dir + `partclone \| compressor \| split` | `src-tauri/src/backup.rs` |
| `restore_manager.py` (non-clone) | `sfdisk` table write + `cat \| decompress \| partclone.restore` | `src-tauri/src/restore.rs` |
| `parser/clonezilla_image.py` | scan/describe image directories | `restore.rs::scan_images` |
| `utility.py` (compression/progress helpers) | compressor tables, byte-weighted progress | `src-tauri/src/pipeline.rs` + `clone.rs` |

### The clone wizard (7 pages)

1. **Welcome** — intro
2. **Source drive** — pick the disk to copy from (read-only)
3. **Destination drive** — pick the disk to overwrite
4. **Partitions** — map source→destination partitions; optionally overwrite the partition table
5. **Confirm** — review, rescue mode, post-op action, then an "are you sure" modal
6. **Cloning** — live byte-weighted progress + partclone activity log
7. **Summary** — per-partition results

### The backup wizard (7 pages)

Source drive → partitions → destination folder → name & compression (zstd/gzip/uncompressed)
→ confirm → progress → summary. Writes a Clonezilla-compatible image directory
(`parts`, `disk`, `<disk>-pt.sf`, `<disk>-mbr`, `dev-fs.list`, and
`<part>.<fs>-ptcl-img.<comp>.aa…` split image files).

### The restore wizard (6 pages)

Image folder → select image → destination drive → confirm (+ overwrite-table / rescue)
→ progress → summary. Scans a folder for image directories (identified by the
`parts` file) and shows each image's date, source disk, partitions, sizes, and
compression.

## Architecture

- **Frontend** — React 18 + TypeScript + Vite + Tailwind. UI components are
  copied from beUI (`src/components/motion/*`, MIT licence): `button`,
  `checkbox`, `select`, `center-morph-modal`, plus the shared `lib/ease.ts`,
  `lib/utils.ts`, and listbox hooks. The wizard-specific components
  (`WizardChrome`, `DriveTable`, `ProgressBar`, `PartitionMappingTable`, toasts)
  are built in the beUI idiom (Motion springs, shared-layout pills). `App.tsx`
  is a thin router over a `Home` screen and the three wizards in `src/wizards/`.
- **Backend** — a Rust Tauri crate (`src-tauri/`) exposing commands:
  `preflight`, `list_drives`, `scan_images`, `list_dirs`, `start_clone`,
  `start_backup`, `start_restore` (all stream `clone://progress` events), and
  `cancel_operation`. The real pipelines (`lsblk` → `sfdisk`/`sgdisk` →
  `partclone`) only run on **Linux with root**; otherwise the commands return
  `UnsupportedPlatform`. A `preflight` check (`preflight.rs`) verifies the
  platform, root privilege, and required tools, and gates every real operation.
- **CLI harness** — `rabobi` (`src-tauri/src/bin/cli.rs`) runs the same
  engine headlessly (`list-drives`, `scan-images`, `clone`, `backup`, `restore`)
  and backs the Linux loopback integration tests in `scripts/`.
- **Simulation mode** — when the native backend is absent (plain `vite dev`, or
  macOS/Windows), `src/lib/clone/simulate.ts` provides fake drives, a simulated
  image library, and faithful byte-weighted progress so the whole UX is usable
  for development and demos.

> ⚠️ **Real operations are destructive.** The destination disk is permanently
> overwritten. On **Linux** the engine uses partclone/sfdisk/lsblk and must run
> as root; on **Windows** it uses PowerShell enumeration + raw `\\.\PhysicalDrive`
> copy and must run as Administrator (see [docs/WINDOWS.md](docs/WINDOWS.md)).
> macOS and plain `vite dev` run in safe **simulation mode**. Always
> double-check the target device.

## Develop

```bash
npm install

# Web-only (simulation mode), fast UI iteration:
npm run dev            # http://localhost:1420

# Full desktop app (requires the Tauri prerequisites for your OS):
npm run app:dev

# Checks
npx tsc --noEmit                   # typecheck frontend
npm run build                      # build web bundle
cd src-tauri && cargo test         # Rust unit tests (parsers, progress, pipeline)
cd src-tauri && cargo check --all-targets
```

## Verify on Linux (safe loopback harness)

See [docs/LINUX_TESTING.md](docs/LINUX_TESTING.md). In short, on a Linux host:

```bash
sudo ./scripts/make-test-disks.sh      # two throwaway loop "disks"
sudo ./scripts/test-list-drives.sh     # enumerate them
sudo ./scripts/test-clone.sh           # clone + verify with diff
sudo ./scripts/test-backup-restore.sh  # backup then restore + verify
sudo ./scripts/cleanup-test-disks.sh   # tear down
```

Nothing touches a real disk — the scripts only act on the loop devices they
created (recorded in `_testdisks.env`).

## Licence / attribution

- UI components are from **beUI** (https://beui.dev, MIT) by Saurabh Chauhan.
- The clone workflow and data model are reverse-engineered from **Rescuezilla**
  (https://github.com/rescuezilla/rescuezilla, GPL). This is an independent
  reimplementation of its behaviour, not a copy of its source.
