#!/usr/bin/env bash
# Back up the source loop disk to an image directory, then restore that image
# to the destination loop disk and verify.
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
source "$HERE/_testdisks.env"

if [[ "$(id -u)" != "0" ]]; then echo "Run as root." >&2; exit 1; fi

CLI="$HERE/src-tauri/target/release/rabobi"
[[ -x "$CLI" ]] || ( cd "$HERE/src-tauri" && cargo build --release --bin rabobi )

IMG_DIR="$HERE/_testout/image"
rm -rf "$HERE/_testout"; mkdir -p "$IMG_DIR"

echo "==> Backup $RC_TEST_SRC -> $IMG_DIR (zstd)"
"$CLI" backup --source "$RC_TEST_SRC" --dest-dir "$IMG_DIR" --compression zstd

echo "==> Image directory contents:"
ls -la "$IMG_DIR" | sed 's/^/    /'

echo "==> Scan image"
"$CLI" scan-images --dir "$HERE/_testout" | sed 's/^/    /'

echo "==> Restore $IMG_DIR -> $RC_TEST_DST (overwrite table)"
"$CLI" restore --image "$IMG_DIR" --dest "$RC_TEST_DST" --overwrite-table

partprobe "$RC_TEST_DST" || true; sleep 1
SRC_P1="${RC_TEST_SRC}p1"; [[ -b "$SRC_P1" ]] || SRC_P1="${RC_TEST_SRC}1"
DST_P1="${RC_TEST_DST}p1"; [[ -b "$DST_P1" ]] || DST_P1="${RC_TEST_DST}1"

M1="$(mktemp -d)"; M2="$(mktemp -d)"
mount -o ro "$SRC_P1" "$M1"; mount -o ro "$DST_P1" "$M2"
rc=0
if diff -r "$M1" "$M2" >/dev/null; then
  echo "PASS: restored filesystem contents identical"
else
  echo "FAIL: contents differ" >&2; rc=1
fi
umount "$M1"; umount "$M2"; rmdir "$M1" "$M2"
exit $rc
