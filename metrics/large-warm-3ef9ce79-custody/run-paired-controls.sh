#!/bin/bash
set -u
proof=/workspace/large-warm-proof
root=/workspace/perf-011-large
base=ec5ee1605212be5a27fd8b5e145d1cb6cfa41718
TIMEFORMAT='elapsed_seconds=%R user_seconds=%U system_seconds=%S'
for label in A1 B0 B1 A2 B2 A3 B3 BCold; do
  binary=$proof/bin/ripr-repair
  if [[ "$label" = A* ]]; then binary=/workspace/perf-011-evidence/bin/ripr; fi
  export RIPR_REPO_EXPOSURE_LATENCY_TRACE=1
  export RIPR_CACHE_DIR=/workspace/large-warm-cache RIPR_CHECK_FINDINGS_BYTES=0 RIPR_REPO_EXPOSURE_LATENCY_RECEIPT_TOKEN=$label
  if test "$label" = BCold; then export RIPR_CACHE_DIR=/workspace/indirect-owner-cold; fi
  { time timeout --signal=TERM --kill-after=5s 90s "$binary" check --root "$root" --base "$base" --mode draft --format json > "$proof/$label.json" 2> "$proof/$label.stderr"; native_status=$?; printf '%s\n' "$native_status" > "$proof/$label.status"; } 2> "$proof/$label.time"
  if test "$native_status" != 0; then exit "$native_status"; fi
done
