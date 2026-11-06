#!/usr/bin/env bash
# Deterministic shell controls; uses disposable git fixtures and a fake cargo.
set -euo pipefail
mode="${1:-all}"
bench="$(cd "$(dirname "$0")" && pwd)"
root="$(mktemp -d "${TMPDIR:-/tmp}/agent-bench-test.XXXXXX")"
trap 'touch "$root/release"; rm -rf "$root"' EXIT
mkdir -p "$root/bench" "$root/bin"
cp "$bench/record-run.sh" "$bench/eval.sh" "$root/bench/"

records() {
  mkdir -p "$root/results"
  printf 'context\n' > "$root/context"
  printf 'fn example() {}\n' > "$root/test.rs"
  printf 'DISCRIMINATES: yes\n' > "$root/eval"
  local real_mv
  real_mv="$(command -v mv)"
  cat > "$root/bin/mv" <<'WRAPPER'
#!/usr/bin/env bash
if [[ ${ISOLATION_PAUSE:-} == 1 && ${*: -1} == */summary.csv ]]; then
  touch "$ISOLATION_ROOT/paused"
  for i in {1..500}; do [[ -e "$ISOLATION_ROOT/release" ]] && break; sleep .01; done
  [[ -e "$ISOLATION_ROOT/release" ]] || exit 98
fi
if [[ ${ISOLATION_FAIL:-} == 1 && ${*: -1} == */summary.csv ]]; then exit 17; fi
exec "$ISOLATION_MV" "$@"
WRAPPER
  chmod +x "$root/bin/mv"
  export ISOLATION_ROOT="$root" ISOLATION_MV="$real_mv" PATH="$root/bin:$PATH"
  record() {
    local id="$1"; shift
    bash "$root/bench/record-run.sh" --results-dir "$root/results" --run-id "inserted-guard-vanilla-r$id" --scenario inserted-guard --variant vanilla --context-file "$root/context" --rounds 1 --test-name example --discriminates yes --eval-output "$root/eval" --final-test "$root/test.rs" "$@"
  }
  ISOLATION_PAUSE=1 record 01 > "$root/first.log" 2>&1 &
  local pid=$! rc=0
  for i in {1..500}; do [[ -e "$root/paused" ]] && break; sleep .01; done
  [[ -e "$root/paused" ]] || { echo 'first writer did not reach summary rename' >&2; return 1; }
  record 02 > "$root/second.log" 2>&1 || rc=$?
  touch "$root/release"
  wait "$pid"
  [[ "$rc" == 2 ]] && grep -q 'another record writer' "$root/second.log" || { echo "overlapping writer was not refused: exit $rc" >&2; return 1; }
  [[ ! -e "$root/results/inserted-guard-vanilla-r02.md" ]]
  record 02 > /dev/null
  [[ $(find "$root/results" -maxdepth 1 -name '*.md' | wc -l) -eq 2 ]]
  [[ $(wc -l < "$root/results/summary.csv") -eq 3 ]]
  local before
  before="$(sha256sum "$root/results/summary.csv")"
  mkdir "$root/results/.record-run.lock"
  rc=0; record 03 > "$root/stale.log" 2>&1 || rc=$?
  [[ "$rc" == 2 && -d "$root/results/.record-run.lock" ]]
  [[ ! -e "$root/results/inserted-guard-vanilla-r03.md" ]]
  [[ "$before" == "$(sha256sum "$root/results/summary.csv")" ]]
  rmdir "$root/results/.record-run.lock"
  local report_before
  report_before="$(sha256sum "$root/results/inserted-guard-vanilla-r01.md")"
  rc=0; record 01 > "$root/duplicate.log" 2>&1 || rc=$?
  [[ "$rc" == 2 ]] && grep -q 'report already exists' "$root/duplicate.log"
  [[ "$report_before" == "$(sha256sum "$root/results/inserted-guard-vanilla-r01.md")" ]]
  rc=0; record 03 --rounds 0 > /dev/null 2>&1 || rc=$?
  [[ "$rc" == 2 && ! -e "$root/results/inserted-guard-vanilla-r03.md" ]]
  : > "$root/empty-context"
  rc=0; record 03 --context-file "$root/empty-context" > /dev/null 2>&1 || rc=$?
  [[ "$rc" == 2 && ! -e "$root/results/inserted-guard-vanilla-r03.md" ]]
  rc=0; record 03 --discriminates no > /dev/null 2>&1 || rc=$?
  [[ "$rc" == 2 && ! -e "$root/results/inserted-guard-vanilla-r03.md" ]]
  rc=0; ISOLATION_FAIL=1 record 03 > /dev/null 2>&1 || rc=$?
  [[ "$rc" == 17 && -f "$root/results/inserted-guard-vanilla-r03.md" && ! -e "$root/results/.record-run.lock" ]]
  [[ "$before" == "$(sha256sum "$root/results/summary.csv")" ]]
  record 04 > /dev/null
  [[ $(wc -l < "$root/results/summary.csv") -eq 5 ]]
  grep -Fxq 'inserted-guard-vanilla-r03,inserted-guard,vanilla,8,1,yes' "$root/results/summary.csv"
  grep -Fxq 'inserted-guard-vanilla-r04,inserted-guard,vanilla,8,1,yes' "$root/results/summary.csv"
  cp -a "$bench/results/seed-study" "$root/seed"
  local seed_before
  seed_before="$(sha256sum "$root/seed/inserted-guard-vanilla-r01.md")"
  record 99 --results-dir "$root/seed" > /dev/null
  [[ $(wc -l < "$root/seed/summary.csv") -eq 7 ]]
  [[ "$seed_before" == "$(sha256sum "$root/seed/inserted-guard-vanilla-r01.md")" ]]
  for f in "$bench"/results/seed-study/*-r*.md; do
    local id
    id="$(sed -n 's/^run_id: //p' "$f")"
    [[ -n "$id" ]] && grep -q "^$id," "$root/seed/summary.csv"
  done
  echo 'record writer, refusals, and interrupted-summary recovery: PASS'
}

logs() {
  mkdir -p "$root/source/src" "$root/source/tests" "$root/one" "$root/two"
  git -C "$root/source" init -q
  printf 'original\n' > "$root/source/src/lib.rs"
  git -C "$root/source" add src/lib.rs
  git -C "$root/source" -c user.name=test -c user.email=test@example.invalid commit -qm original
  printf 'changed\n' > "$root/source/src/lib.rs"
  git -C "$root/source" add src/lib.rs
  git -C "$root/source" -c user.name=test -c user.email=test@example.invalid commit -qm changed
  git clone -q "$root/source" "$root/one/case"
  git clone -q "$root/source" "$root/two/case"
  for crate in "$root/one/case" "$root/two/case"; do
    mkdir -p "$crate/tests"
    printf 'fn example() { assert!(true); }\n' > "$crate/tests/example.rs"
  done
  cat > "$root/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
if [[ ${1:-} == --version ]]; then echo 'cargo fixture'; exit 0; fi
printf 'fixture crate: %s\n' "$PWD"
if [[ $(cat src/lib.rs) == original ]]; then echo 'test example ... ok'; exit 0; fi
echo 'test example ... FAILED'
exit 101
CARGO
  chmod +x "$root/bin/cargo"
  export PATH="$root/bin:$PATH" AGENT_BENCH_ALLOW_TEMP=1
  bash "$root/bench/eval.sh" "$root/one/case" example > "$root/one.out" 2>&1 & local a=$!
  bash "$root/bench/eval.sh" "$root/two/case" example > "$root/two.out" 2>&1 & local b=$!
  wait "$a"; wait "$b"
  for side in one two; do
    grep -q 'PHASE changed(HEAD):.*exit=101' "$root/$side.out"
    grep -q 'DISCRIMINATES: yes' "$root/$side.out"
    for phase in original changed; do
      grep -Fxq "fixture crate: $root/$side/case" "$root/$side/case/.agent-bench-logs/eval-$phase.log"
    done
    [[ $(cat "$root/$side/case/src/lib.rs") == changed ]]
    git -C "$root/$side/case" diff --quiet
  done
  echo 'run-owned phase logs and Cargo exits: PASS'
}

case "$mode" in
  records) records ;;
  logs) logs ;;
  all) records; logs ;;
  *) echo 'Usage: bash test-isolation.sh [records|logs|all]' >&2; exit 2 ;;
esac
