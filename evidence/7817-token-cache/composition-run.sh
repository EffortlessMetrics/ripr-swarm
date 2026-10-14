#!/bin/bash
set -u
label=$1
shift
cd /workspace/7817-composition
export PATH=/workspace/.cargo/bin:$PATH CARGO_HOME=/workspace/.cargo RUSTUP_HOME=/workspace/.rustup CARGO_TARGET_DIR=/workspace/7817-target
receipt=/workspace/7817-repair-evidence/$label.receipt
{ date -u +%FT%TZ; git rev-parse HEAD HEAD^{tree}; git diff --binary | sha256sum; printf '%q ' "$@"; printf '\n'; } > "$receipt"
"$@" > /workspace/7817-repair-evidence/$label.log 2>&1
code=$?
printf 'exit=%s\n' "$code" >> "$receipt"
date -u +%FT%TZ >> "$receipt"
tail -12 /workspace/7817-repair-evidence/$label.log
cat "$receipt"
exit "$code"
