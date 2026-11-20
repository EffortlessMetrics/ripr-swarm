#!/usr/bin/env bash
# Run with a fresh directory outside Cargo workspaces; retain its evidence.
set -euo pipefail
[ "$#" -eq 1 ] || { echo "usage: bash test-concurrency.sh <fresh-directory>" >&2; exit 2; }
root="$1"
[ ! -e "$root" ] || { echo "directory already exists: $root" >&2; exit 2; }
mkdir -p "$root"
root="$(cd "$root" && pwd)"
bench="$(cd "$(dirname "$0")" && pwd)"

# Freeze writer 1 after it snapshots reports but before publishing summary.
# A second writer must refuse while writer 1 holds the lock, then succeed on retry.
mkdir "$root/bin" "$root/results"
real_mv="$(command -v mv)"
cat > "$root/bin/mv" <<'WRAPPER'
#!/usr/bin/env bash
if [ "${BLOCK_SUMMARY:-0}" = 1 ] && [[ "${*: -1}" = */summary.csv ]]; then
  : > "$SYNC_ROOT/blocked"
  for _ in {1..300}; do [ -e "$SYNC_ROOT/release" ] && break; sleep .1; done
  [ -e "$SYNC_ROOT/release" ] || exit 1
fi
exec "$REAL_MV" "$@"
WRAPPER
chmod +x "$root/bin/mv"
printf 'context\n' > "$root/context"
printf 'fn probe() {}\n' > "$root/test.rs"
probe_hash="$(sha256sum "$root/test.rs" | awk '{print $1}')"
printf 'DISCRIMINATES: yes\nTEST_SOURCE_SHA256: %s\n' "$probe_hash" > "$root/eval"
record() {
  bash "$bench/record-run.sh" --results-dir "$root/results" --run-id "inserted-guard-vanilla-r$1" \
    --scenario inserted-guard --variant vanilla --context-file "$root/context" --rounds 1 \
    --test-name probe --discriminates yes --eval-output "$root/eval" --final-test "$root/test.rs"
}
export REAL_MV="$real_mv" SYNC_ROOT="$root" PATH="$root/bin:$PATH"
BLOCK_SUMMARY=1 record 01 > "$root/first.log" 2>&1 & first=$!
for _ in {1..300}; do [ -e "$root/blocked" ] && break; sleep .1; done
[ -e "$root/blocked" ] || { echo 'writer did not reach summary publication' >&2; exit 1; }
rc=0; record 02 > "$root/second.log" 2>&1 || rc=$?
: > "$root/release"
wait "$first"
[ "$rc" -eq 2 ] || { echo "overlapping writer returned $rc, expected 2" >&2; exit 1; }
grep -q 'cannot acquire' "$root/second.log"
record 02 > "$root/retry.log" 2>&1
[ "$(wc -l < "$root/results/summary.csv")" -eq 3 ]
grep -q '^inserted-guard-vanilla-r01,' "$root/results/summary.csv"
grep -q '^inserted-guard-vanilla-r02,' "$root/results/summary.csv"
[ ! -e "$root/results/.record-run.lock" ]
before="$(sha256sum "$root/results/inserted-guard-vanilla-r02.md")"
rc=0; record 02 > "$root/duplicate.log" 2>&1 || rc=$?
[ "$rc" -eq 2 ] || { echo "duplicate run returned $rc, expected 2" >&2; exit 1; }
grep -q 'report already exists' "$root/duplicate.log"
[ "$before" = "$(sha256sum "$root/results/inserted-guard-vanilla-r02.md")" ]
[ ! -e "$root/results/.record-run.lock" ]

# Same basename in two run roots must not share phase logs. Different tests
# produce different verdicts; run them concurrently using real Cargo.
bash "$bench/make-scenario-crates.sh" "$root/run-a" > "$root/materialize-a.log"
bash "$bench/make-scenario-crates.sh" "$root/run-b" > "$root/materialize-b.log"
a="$root/run-a/inserted-guard"; b="$root/run-b/inserted-guard"
cat > "$a/tests/probe.rs" <<'TEST'
#[test]
fn probe() { assert_eq!(inserted_guard::total_after_fee(-100, 5), -105); }
TEST
cat > "$b/tests/probe.rs" <<'TEST'
#[test]
fn probe() { assert_eq!(inserted_guard::total_after_fee(100, 5), 95); }
TEST
bash "$bench/eval.sh" "$a" probe > "$root/eval-a.log" 2>&1 & pa=$!
bash "$bench/eval.sh" "$b" probe > "$root/eval-b.log" 2>&1 & pb=$!
rca=0; wait "$pa" || rca=$?
rcb=0; wait "$pb" || rcb=$?
[ "$rca" -eq 0 ] || { echo "first evaluator returned $rca" >&2; exit 1; }
grep -q '^DISCRIMINATES: yes$' "$root/eval-a.log"
[ "$rcb" -eq 1 ] || { echo "second evaluator returned $rcb" >&2; exit 1; }
grep -q '^DISCRIMINATES: no$' "$root/eval-b.log"
grep -q 'test probe ... FAILED' "$root/run-a/.agent-bench-logs/inserted-guard/eval-changed.log"
grep -q 'test probe ... ok' "$root/run-b/.agent-bench-logs/inserted-guard/eval-changed.log"
echo "Concurrency controls passed; evidence at $root"
