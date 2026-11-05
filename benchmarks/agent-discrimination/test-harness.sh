#!/usr/bin/env bash
# Focused harness controls. Supply a fresh directory outside Cargo workspaces
# and shared temp, and an executable ripr binary. This retains the evidence.
set -euo pipefail
[ "$#" -eq 2 ] || { echo "Usage: bash test-harness.sh <fresh-dir> <ripr-bin>" >&2; exit 2; }
bench="$(cd "$(dirname "$0")" && pwd)"
root="$1"; ripr="$2"
[ ! -e "$root" ] || { echo "ERROR: test directory already exists: $root" >&2; exit 2; }
mkdir -p "$root"
root="$(cd "$root" && pwd)"
bash "$bench/make-scenario-crates.sh" "$root/crates"
crate="$root/crates/inserted-guard"
cat > "$crate/tests/a.rs" <<'TEST'
#[test]
fn same_name() { assert_eq!(inserted_guard::total_after_fee(100, 5), 95); }
TEST
cat > "$crate/tests/b.rs" <<'TEST'
#[test]
fn same_name() { assert_eq!(inserted_guard::total_after_fee(-100, 5), -105); }
TEST
run_expect() {
  local expected="$1"; shift
  local rc=0
  "$@" > "$root/output.log" 2>&1 || rc=$?
  [ "$rc" -eq "$expected" ] || { cat "$root/output.log"; echo "expected exit $expected, got $rc" >&2; exit 1; }
}
run_expect 2 bash "$bench/eval.sh" "$root/missing-crate" same_name
grep -q "crate path not found" "$root/output.log"
run_expect 2 bash "$bench/eval.sh" "$crate" same_name
grep -q 'found 2' "$root/output.log"
mv "$crate/tests/a.rs" "$root/a.rs"
run_expect 0 bash "$bench/eval.sh" "$crate" same_name
grep -q 'DISCRIMINATES: yes' "$root/output.log"
cp "$root/output.log" "$root/eval-positive.log"
printf 'context\n' > "$root/context.txt"
record=(bash "$bench/record-run.sh" --results-dir "$root/results" --run-id inserted-guard-vanilla-r01 --scenario inserted-guard --variant vanilla --context-file "$root/context.txt" --rounds 1 --test-name same_name --discriminates yes --eval-output "$root/eval-positive.log")
run_expect 2 "${record[@]}" --final-test "$root/a.rs"
run_expect 0 "${record[@]}" --final-test "$crate/tests/b.rs"
mkdir "$root/results/.record-run.lock"
run_expect 2 "${record[@]}" --final-test "$crate/tests/b.rs"
rmdir "$root/results/.record-run.lock"
run_expect 2 "${record[@]}" --final-test "$crate/tests/b.rs"
grep -q 'report already exists' "$root/output.log"
mv "$crate/tests/b.rs" "$root/b.rs"
mv "$root/a.rs" "$crate/tests/a.rs"
run_expect 1 bash "$bench/eval.sh" "$crate" same_name
grep -q 'DISCRIMINATES: no' "$root/output.log"
mkdir -p "$root/nested"
printf '[workspace]\nmembers = []\n' > "$root/nested/Cargo.toml"
cp -a "$crate" "$root/nested/inserted-guard"
run_expect 2 bash "$bench/build-contexts.sh" "$root/nested/inserted-guard" "$ripr" "$root/refused" inserted-guard test-r01
[ ! -e "$root/refused" ]
run_expect 0 bash "$bench/build-contexts.sh" "$crate" "$ripr" "$root/new-context" inserted-guard test-r01
[ -s "$root/new-context/vanilla.txt" ]
grep -q 'explain captured: yes' "$root/output.log"
[ -s "$root/new-context/ripr-explain.txt" ]
mkdir -p "$root/space parent"
cp -a "$crate" "$root/space parent/inserted-guard"
run_expect 0 bash "$bench/build-contexts.sh" "$root/space parent/inserted-guard" "$ripr" "$root/space output" inserted-guard test-r01
grep -q 'explain captured: yes' "$root/output.log"
[ -s "$root/space output/ripr-explain.txt" ]
cat > "$root/failing-ripr" <<'STUB'
#!/usr/bin/env bash
if [ "$1" = --version ]; then echo test-ripr; exit 0; fi
echo "forced check failure" >&2
exit 2
STUB
chmod +x "$root/failing-ripr"
run_expect 2 bash "$bench/build-contexts.sh" "$crate" "$root/failing-ripr" "$root/failed-check" inserted-guard test-r01
grep -q 'ripr check failed' "$root/output.log"
# Simulate a root Cargo.toml without writing to the host filesystem root.
# The real scripts still walk their actual paths; only the root file test is
# injected. Windows drive-root behavior needs a native Git Bash control.
cat > "$root/mock-root-test.sh" <<'MOCK'
[() {
  if [[ "${1:-}" == -f && "${2:-}" == //Cargo.toml ]]; then return 0; fi
  builtin [ "$@"
}
MOCK
run_expect 2 env BASH_ENV="$root/mock-root-test.sh" bash "$bench/make-scenario-crates.sh" "$root/root-generator"
grep -q '//Cargo.toml' "$root/output.log"
run_expect 2 env BASH_ENV="$root/mock-root-test.sh" bash "$bench/eval.sh" "$crate" same_name
grep -q '//Cargo.toml' "$root/output.log"
run_expect 2 env BASH_ENV="$root/mock-root-test.sh" bash "$bench/build-contexts.sh" "$crate" "$ripr" "$root/root-context" inserted-guard test-r01
grep -q '//Cargo.toml' "$root/output.log"
echo "Harness controls passed; artifacts retained at $root"
