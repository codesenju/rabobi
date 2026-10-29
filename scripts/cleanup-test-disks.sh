#!/usr/bin/env bash
# Detach the loopback test disks created by make-test-disks.sh and remove temp
# files. Safe: only touches devices recorded in _testdisks.env.
set -euo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="$HERE/_testdisks.env"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "No _testdisks.env found; nothing to clean up."
  exit 0
fi
# shellcheck disable=SC1090
source "$ENV_FILE"

if [[ "$(id -u)" != "0" ]]; then
  echo "Run as root (sudo)." >&2
  exit 1
fi

for dev in "${RC_TEST_SRC:-}" "${RC_TEST_DST:-}"; do
  [[ -n "$dev" ]] || continue
  # unmount any children first
  for part in $(lsblk -nro PATH "$dev" 2>/dev/null | tail -n +2); do
    umount "$part" 2>/dev/null || true
  done
  losetup -d "$dev" 2>/dev/null && echo "detached $dev" || echo "already detached $dev"
done

rm -f "${RC_TEST_SRC_IMG:-}" "${RC_TEST_DST_IMG:-}"
rm -rf "$HERE/_testdisks" "$HERE/_testout"
rm -f "$ENV_FILE"
echo "Cleanup complete."
