#!/usr/bin/env bash
# eval.sh -- agent-discrimination A/B evaluator: does the new test
# discriminate the change?
#
# Usage:
#   bash eval.sh <crate_path> <test_function_name>
#
# Protocol (fixed direction, matches RUNBOOK.md):
#   Phase 1 (ORIGINAL): flip the crate's changed source files back to
#            HEAD~1 content, run the exact integration test -> MUST PASS
#            (exit 0 AND an exact `test <name> ... ok` line).
#   Phase 2 (CHANGED): restore HEAD content, run the exact integration test
#            -> MUST FAIL (nonzero exit AND an exact
#            `test <name> ... FAILED` line).
#
# The test name must be defined in exactly one top-level tests/*.rs
# integration target; the phases run that target with an exact name filter so
# the verdict and the displayed inputs are bound to one source file.
#
# Prints DISCRIMINATES: yes|no, a SHA-256 digest of the test source used, plus
# the assert lines (inputs) the test uses.
# Phase logs are kept in a run-owned directory next to the crate, never
# inside it: a submitted test must not be able to observe (or be scored by)
# evaluator artifacts under the crate. Exit code 0 iff DISCRIMINATES: yes.
#
# Validity fixes over the Nov 2-4 seed-study evaluator (issue #13113):
#   - step-zero toolchain PATH normalization (cargo was off the harness PATH
#     in 9/9 seed-study runs);
#   - cargo's true exit codes are captured inside run_phase and reported
#     (the seed-study version's `RC=$?` after an assignment swallowed them);
#   - refuses to evaluate a crate materialized in shared temp space (one
#     writer per run-owned clone; concurrent-writer collisions contaminated
#     4-5 of 9 seed-study runs). Override with AGENT_BENCH_ALLOW_TEMP=1;
#     any override is disclosed on stdout.
set -u
export CARGO_TERM_COLOR=never

if [ $# -ne 2 ]; then
  echo "Usage: bash eval.sh <crate_path> <test_function_name>"
  exit 2
fi

if [ ! -d "$1" ]; then
  echo "ERROR: crate path not found: $1"
  exit 2
fi
CRATE="$(cd "$1" && pwd -W 2>/dev/null || pwd)"
TEST_NAME="$2"
# Run-owned log directory OUTSIDE the crate: the evaluator's phase logs are
# not crate content, and a submitted test must not be able to inspect their
# presence or absence inside the crate (a test scoring on the log's absence
# would fake discrimination). One writer per run-owned clone, so a
# crate-named subdirectory under the sibling logs root cannot collide.
LOG_DIR="$(dirname "$CRATE")/.agent-bench-logs/$(basename "$CRATE")"
LOG_ORIG="$LOG_DIR/eval-original.log"
LOG_CHNG="$LOG_DIR/eval-changed.log"

# ---- Step zero: toolchain PATH normalization (issue #13113) -----------------
if ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not on PATH and $HOME/.cargo/bin/cargo not found"; exit 2; }
echo "TOOLCHAIN: cargo at $(command -v cargo) ($(cargo --version 2>/dev/null | head -n 1))"

# ---- Shared-temp prohibition (issue #13113) ---------------------------------
LOWER="$(printf '%s' "$CRATE" | tr '[:upper:]' '[:lower:]')"
case "$LOWER" in
  ?:\/temp|?:\/temp/*|*/temp/*|/f/temp|/f/temp/*|/tmp|/tmp/*|/var/tmp|/var/tmp/*)
    if [ "${AGENT_BENCH_ALLOW_TEMP:-}" = "1" ]; then
      echo "WARNING: AGENT_BENCH_ALLOW_TEMP=1 -- evaluating a crate in shared temp space: $CRATE (runs must not share scenario crates; see RUNBOOK.md)"
    else
      echo "ERROR: refusing to evaluate a crate in shared temp space: $CRATE"
      echo "       Materialize a run-owned clone outside shared temp (RUNBOOK.md),"
      echo "       or set AGENT_BENCH_ALLOW_TEMP=1 to override (disclosed here)."
      exit 2
    fi ;;
esac

git -C "$CRATE" rev-parse --verify -q HEAD >/dev/null || { echo "ERROR: $CRATE is not a git repo"; exit 2; }
git -C "$CRATE" rev-parse --verify -q HEAD~1 >/dev/null || { echo "ERROR: $CRATE has no HEAD~1 (need 2 commits)"; exit 2; }

# Conservatively refuse any ancestor Cargo manifest, including the filesystem
# root: a workspace ancestor can claim the nested package and break
# evaluation; some non-workspace manifests would be safe, but the scripts
# refuse them all rather than guess. The manifest check runs BEFORE the
# root-pattern break so a root Cargo.toml is still detected.
DIR_WALK="$CRATE"
while :; do
  if [ -f "$DIR_WALK/Cargo.toml" ] && [ "$DIR_WALK" != "$CRATE" ]; then
    echo "ERROR: crate '$CRATE' sits under Cargo manifest '$DIR_WALK/Cargo.toml'; an ancestor workspace may claim it." >&2
    echo "       Materialize run-owned clones outside any ancestor Cargo manifest (see RUNBOOK.md)." >&2
    exit 2
  fi
  case "$DIR_WALK" in [A-Za-z]:|[A-Za-z]:/|/|.) break ;; esac
  PARENT="$(dirname "$DIR_WALK")"
  [ "$PARENT" = "$DIR_WALK" ] && break
  DIR_WALK="$PARENT"
done

CHANGED_FILES="$(git -C "$CRATE" diff --name-only HEAD~1 HEAD)"
[ -n "$CHANGED_FILES" ] || { echo "ERROR: no tracked file differs between HEAD~1 and HEAD in $CRATE"; exit 2; }

if ! git -C "$CRATE" diff --quiet -- $CHANGED_FILES; then
  echo "ERROR: uncommitted edits to changed source file(s): $CHANGED_FILES"
  echo "       The runbook forbids modifying src/; reset the crate and put the test in tests/."
  exit 2
fi

# Cargo can run identically named tests in multiple integration targets. A
# name alone cannot bind its verdict and displayed input to one source file,
# so the test name must be a plain identifier defined in exactly one
# top-level tests/*.rs integration target (no nested test files).
# Limitation: the fn-name search below is textual, so a `fn <name>(` inside a
# comment, string, or #[cfg]-gated item also counts; a nested-mod function
# additionally requires a module::name filter this protocol does not use.
# Both cases fail safe (phase 1 cannot produce the exact ok line), they can
# only score `no`, never credit a wrong verdict.
case "$TEST_NAME" in
  ''|*[!a-zA-Z0-9_]*) echo "ERROR: test name must be a Rust function identifier"; exit 2 ;;
esac
TEST_FILES="$(grep -RlF "fn ${TEST_NAME}(" "$CRATE/tests" 2>/dev/null || true)"
TEST_COUNT="$(printf '%s\n' "$TEST_FILES" | awk 'NF { n++ } END { print n+0 }')"
if [ "$TEST_COUNT" -ne 1 ]; then
  echo "ERROR: expected exactly one test file defining fn ${TEST_NAME}( under $CRATE/tests; found $TEST_COUNT"
  exit 2
fi
TEST_FILE="$TEST_FILES"
# The protocol uses a top-level integration target, not a unit test or an
# included helper module with an unrelated Cargo target name. The nested
# pattern comes first: in case globs `*` also matches `/`, so a nested file
# would otherwise satisfy the top-level pattern.
case "$TEST_FILE" in
  "$CRATE"/tests/*/*) echo "ERROR: nested test files are not supported"; exit 2 ;;
  "$CRATE"/tests/*.rs) ;;
  *) echo "ERROR: test must be in a top-level tests/*.rs integration target"; exit 2 ;;
esac
TEST_TARGET="${TEST_FILE##*/}"
TEST_TARGET="${TEST_TARGET%.rs}"

restore_changed() {
  git -C "$CRATE" checkout -q HEAD -- $CHANGED_FILES
}
mkdir -p "$LOG_DIR" || { echo "ERROR: cannot create $LOG_DIR" >&2; exit 2; }
trap restore_changed EXIT

# Sets PHASE_RC to cargo's true exit status. The caller must read $PHASE_RC,
# NOT $? : the trailing assignment inside this function makes the function
# itself always exit 0 (the seed-study bug this fixes).
run_phase() { # $1=logpath
  # Open the log BEFORE running cargo and stop if it cannot be opened for
  # writing: a readable-but-unwritable stale log must not let fail_line read
  # an old FAILED line and fake a discriminating verdict.
  local log_fd phase_rc
  exec {log_fd}>"$1" || { echo "ERROR: cannot open $1 for writing" >&2; exit 2; }
  ( cd "$CRATE" && cargo test --color never --test "$TEST_TARGET" -- "$TEST_NAME" --exact ) >&"$log_fd" 2>&1
  phase_rc=$?
  exec {log_fd}>&-
  PHASE_RC=$phase_rc
}

ok_line() { # $1=log ; echoes exact `test <name> ... ok` line, if any
  tr -d '\r' < "$1" | awk -v n="$TEST_NAME" '$1=="test" && $2==n && $4=="ok"' | head -n 1
}
fail_line() { # $1=log ; echoes exact `test <name> ... FAILED` line, if any
  tr -d '\r' < "$1" | awk -v n="$TEST_NAME" '$1=="test" && $2==n && $4=="FAILED"' | head -n 1
}

echo "CRATE: $CRATE"
echo "TEST:  $TEST_NAME"

# ---- Phase 1: ORIGINAL behavior (HEAD~1) -- test must PASS -----------------
git -C "$CRATE" checkout -q HEAD~1 -- $CHANGED_FILES
run_phase "$LOG_ORIG"; RC1=$PHASE_RC
OK1="$(ok_line "$LOG_ORIG")"

# ---- Phase 2: CHANGED behavior (HEAD) -- test must FAIL --------------------
restore_changed
run_phase "$LOG_CHNG"; RC2=$PHASE_RC
FAIL2="$(fail_line "$LOG_CHNG")"
OK2="$(ok_line "$LOG_CHNG")"

echo "PHASE original(HEAD~1): cargo test exit=$RC1; ${OK1:-no 'test $TEST_NAME ... ok' line}"
echo "PHASE changed(HEAD):    cargo test exit=$RC2; ${FAIL2:-no 'test $TEST_NAME ... FAILED' line}"

VERDICT=no
REASON=""
if [ "$RC1" -ne 0 ] || [ -z "$OK1" ]; then
  REASON="test did not pass on ORIGINAL behavior (phase 1); it must."
elif [ -n "$OK2" ] && [ "$RC2" -eq 0 ]; then
  REASON="test also passes on CHANGED behavior (phase 2); it observes no difference."
elif [ -z "$FAIL2" ]; then
  REASON="phase 2 cargo exited nonzero without a 'test $TEST_NAME ... FAILED' line (compile error or harness failure); inspect $LOG_CHNG"
else
  VERDICT=yes
fi

echo "DISCRIMINATES: $VERDICT"
if [ -n "$REASON" ]; then
  echo "REASON: $REASON"
  echo "--- phase 1 log tail:"
  tr -d '\r' < "$LOG_ORIG" | tail -n 20
  echo "--- phase 2 log tail:"
  tr -d '\r' < "$LOG_CHNG" | tail -n 20
fi

# Digest of the exact test source the verdict is bound to; record-run.sh
# re-computes it from --final-test and refuses a mismatch, so a recorded
# report can never credit bytes the evaluator did not run.
echo "TEST_SOURCE_SHA256: $(sha256sum "$TEST_FILE" | awk '{print $1}')"
echo "TEST_INPUT_EVIDENCE (from $(basename "$TEST_FILE")):"
tr -d '\r' < "$TEST_FILE" | sed -n "/fn ${TEST_NAME}(/,/^}/p" | grep -n "assert" | sed 's/^/  /'

restore_changed
if git -C "$CRATE" diff --quiet -- $CHANGED_FILES; then
  echo "RESTORED: yes (changed files back at HEAD)"
else
  echo "RESTORED: NO -- manual fix needed: git -C $CRATE checkout HEAD -- $CHANGED_FILES"
fi

if [ "$VERDICT" = "yes" ]; then exit 0; else exit 1; fi
