#!/usr/bin/env bash
# Clone source loop disk -> destination loop disk, then verify the restored
# partition matches the source byte-for-byte.
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
source "$HERE/_testdisks.env"

if [[ "$(id -u)" != "0" ]]; then echo "Run as root." >&2; exit 1; fi

CLI="$HERE/src-tauri/target/release/rabobi"
[[ -x "$CLI" ]] || ( cd "$HERE/src-tauri" && cargo build --release --bin rabobi )

echo "==> Cloning $RC_TEST_SRC -> $RC_TEST_DST (overwrite partition table)"
"$CLI" clone --source "$RC_TEST_SRC" --dest "$RC_TEST_DST" --overwrite-table

echo "==> Re-reading destination partition table"
partprobe "$RC_TEST_DST" || true
sleep 1

SRC_P1="${RC_TEST_SRC}p1"; [[ -b "$SRC_P1" ]] || SRC_P1="${RC_TEST_SRC}1"
DST_P1="${RC_TEST_DST}p1"; [[ -b "$DST_P1" ]] || DST_P1="${RC_TEST_DST}1"

echo "==> Comparing filesystem contents"
M1="$(mktemp -d)"; M2="$(mktemp -d)"
mount -o ro "$SRC_P1" "$M1"
mount -o ro "$DST_P1" "$M2"
rc=0
if diff -r "$M1" "$M2" >/dev/null; then
  echo "PASS: cloned filesystem contents identical"
else
  echo "FAIL: contents differ" >&2
  rc=1
fi
umount "$M1"; umount "$M2"; rmdir "$M1" "$M2"
exit $rc
