#!/bin/bash
set -u
export CARGO_HOME=/workspace/.cargo RUSTUP_HOME=/workspace/.rustup
root=/workspace/7817-repair-evidence/token-native-controls
for case in iterator_observer add_observer; do
  for variant in original write-removal; do
    source=$root/$case-$variant.rs
    binary=$root/$case-$variant.bin
    /workspace/.cargo/bin/rustc --edition 2024 --test --crate-name token_observer "$source" -o "$binary" > "$root/$case-$variant.compile.log" 2>&1
    compile=$?
    printf 'compile_exit=%s\n' "$compile" > "$root/$case-$variant.receipt"
    if [ "$compile" -ne 0 ]; then exit "$compile"; fi
    "$binary" --test-threads=1 > "$root/$case-$variant.run.log" 2>&1
    run=$?
    printf 'run_exit=%s\n' "$run" >> "$root/$case-$variant.receipt"
    sha256sum "$source" "$binary" >> "$root/$case-$variant.receipt"
    date -u +%FT%TZ >> "$root/$case-$variant.receipt"
    if [ "$variant" = original ] && [ "$run" -ne 0 ]; then exit 1; fi
    if [ "$variant" = write-removal ] && [ "$run" -ne 101 ]; then exit 1; fi
    tail -3 "$root/$case-$variant.run.log"
    cat "$root/$case-$variant.receipt"
  done
done
