#!/bin/bash
set -eu
if [ "$1" = v5-check-fast ]; then
  receipt=/workspace/7817-repair-evidence/cache-closeout-before-v5-policy.receipt
  { date -u +%FT%TZ; printf 'Completed-command boundary before v5-check-fast; sequential Cargo owner only.\n'; du -sh /workspace/7817-target/debug/incremental; df -h /workspace; } > "$receipt"
  rm -rf /workspace/7817-target/debug/incremental
  { printf 'Only reusable task incremental cache removed; source, executables and raw receipts retained.\n'; df -h /workspace; } >> "$receipt"
fi
exec /workspace/7817-repair-evidence/run-original.sh "$@"
