#!/usr/bin/env bash
# Build the rescueclone CLI and list drives; assert our loop devices appear.
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
source "$HERE/_testdisks.env"

echo "==> Building rabobi CLI (release)"
( cd "$HERE/src-tauri" && cargo build --release --bin rabobi )
CLI="$HERE/src-tauri/target/release/rabobi"

echo "==> list-drives"
OUT="$("$CLI" list-drives)"
echo "$OUT" | sed 's/^/    /'

for dev in "$RC_TEST_SRC" "$RC_TEST_DST"; do
  if echo "$OUT" | grep -q "$dev"; then
    echo "PASS: $dev present"
  else
    echo "FAIL: $dev missing from list-drives output" >&2
    exit 1
  fi
done
echo "test-list-drives OK"
