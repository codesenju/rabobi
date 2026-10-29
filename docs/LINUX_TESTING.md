# Testing Rabobi on Linux

Rabobi performs **real, destructive disk operations** only on Linux, as
root, using the Clonezilla/Rescuezilla tool-chain. This guide shows how to
verify the native backend safely using **loopback devices** (disk-image files
exposed as `/dev/loopN`), so no physical disk is ever at risk.

## 1. Prerequisites

Install the tool-chain (Debian/Ubuntu names shown):

```bash
sudo apt-get update
sudo apt-get install -y \
  partclone util-linux gdisk \
  pigz zstd bzip2 \
  coreutils           # split, cat, dd, sync, sha256sum, cmp
```

Rust + Node to build:

```bash
# Rust (https://rustup.rs) and Node 20+
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Tauri Linux system deps:
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev
```

Verify the tools the backend needs:

```bash
for t in lsblk sfdisk sgdisk partclone.dd partclone.ext4 pigz zstd split dd sync; do
  command -v "$t" >/dev/null && echo "ok   $t" || echo "MISS $t"
done
```

## 2. Build & unit tests (no root, safe anywhere on Linux)

```bash
cd rescueclone-desktop
npm install
npm run build                 # frontend
cd src-tauri
cargo test                    # parser unit tests (lsblk JSON, partclone progress)
cargo check
```

## 3. Loopback integration tests (root required, still safe)

These create two throwaway disk-image files, attach them as loop devices, and
run the real backend against them. **Nothing touches your real disks.**

```bash
cd rescueclone-desktop

# Create /dev/loopA (source, 512 MiB, partitioned+formatted) and
# /dev/loopB (destination, 768 MiB, blank). Prints the device names.
sudo ./scripts/make-test-disks.sh

# List drives (should include the two loop devices):
sudo ./scripts/test-list-drives.sh

# Clone source -> destination and verify the copied partition byte-for-byte:
sudo ./scripts/test-clone.sh

# Back up the source to ./_testout/image, then restore it to the destination
# and verify:
sudo ./scripts/test-backup-restore.sh

# Tear down loop devices and temp files:
sudo ./scripts/cleanup-test-disks.sh
```

Each script is **idempotent** and refuses to run against anything that is not a
loop device it created (it records the device names in `_testdisks.env`).

## 4. Running the real GUI

```bash
sudo -E npm run app:dev        # dev build, needs root for real ops
# or build a bundle:
npm run app:build              # produces AppImage/.deb under src-tauri/target
```

When launched **without root**, the app detects it and (per Milestone 5.1)
refuses to start a real operation, falling back to a read-only/simulation view.

## 5. First real use — the safety gate

Before cloning/restoring a disk that matters, do a full dry run on a throwaway
USB stick:

1. `sudo ./scripts/test-clone.sh` passes on loopback.
2. GUI clone of a spare USB → another spare USB, verified by `cmp`.
3. Only then use it on real data, and **always** keep an independent backup.
