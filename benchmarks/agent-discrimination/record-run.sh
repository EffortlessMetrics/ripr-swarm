#!/usr/bin/env bash
# record-run.sh -- write one benchmark run record and rebuild summary.csv.
#
# Validity fixes over the seed study (issue #13113):
#   - structured fields are REQUIRED and validated; context_bytes is measured
#     by this script from the actual context file, never hand-entered (all 9
#     seed-study records carried zeroed fields with the true values only in
#     prose);
#   - the report and summary.csv are written atomically (same-directory
#     temp file + rename);
#   - summary.csv is DERIVED from the report files on every write, so it can
#     include every valid report on a successful rebuild. A crash between the
#     two renames can leave the summary stale until another successful write.
#
# Usage:
#   bash record-run.sh --results-dir <dir> --run-id <scenario>-<variant>-rNN \
#     --scenario <scenario> --variant <vanilla|ripr-context|ripr-tool> \
#     --context-file <path-to-context-or-saved-ripr-output> \
#     --rounds <N> --test-name <fn> --discriminates <yes|no> \
#     --eval-output <file-with-verbatim-eval.sh-output> \
#     --final-test <path-to-tests/discriminate.rs> \
#     --phase-logs-dir <crate>/.agent-bench-logs [--notes "<text>"]
set -euo pipefail

RESULTS_DIR="" RUN_ID="" SCENARIO="" VARIANT="" CONTEXT_FILE="" ROUNDS=""
TEST_NAME="" DISCRIMINATES="" EVAL_OUTPUT="" FINAL_TEST="" PHASE_LOGS_DIR="" NOTES=""

while [ $# -gt 0 ]; do
  case "$1" in
    --results-dir)  RESULTS_DIR="$2";  shift 2 ;;
    --run-id)       RUN_ID="$2";       shift 2 ;;
    --scenario)     SCENARIO="$2";     shift 2 ;;
    --variant)      VARIANT="$2";      shift 2 ;;
    --context-file) CONTEXT_FILE="$2"; shift 2 ;;
    --rounds)       ROUNDS="$2";       shift 2 ;;
    --test-name)    TEST_NAME="$2";    shift 2 ;;
    --discriminates) DISCRIMINATES="$2"; shift 2 ;;
    --eval-output)  EVAL_OUTPUT="$2";  shift 2 ;;
    --final-test)   FINAL_TEST="$2";   shift 2 ;;
    --phase-logs-dir) PHASE_LOGS_DIR="$2"; shift 2 ;;
    --notes)        NOTES="$2";        shift 2 ;;
    *) echo "ERROR: unknown argument: $1" >&2; exit 2 ;;
  esac
done

fail() { echo "ERROR: $*" >&2; exit 2; }

[ -n "$RESULTS_DIR" ]  || fail "--results-dir is required"
[ -n "$RUN_ID" ]       || fail "--run-id is required"
[ -n "$SCENARIO" ]     || fail "--scenario is required"
[ -n "$VARIANT" ]      || fail "--variant is required"
[ -n "$CONTEXT_FILE" ] || fail "--context-file is required"
[ -n "$ROUNDS" ]       || fail "--rounds is required"
[ -n "$TEST_NAME" ]    || fail "--test-name is required"
[ -n "$DISCRIMINATES" ] || fail "--discriminates is required"
[ -n "$EVAL_OUTPUT" ]  || fail "--eval-output is required"
[ -n "$FINAL_TEST" ]   || fail "--final-test is required"
[ -n "$PHASE_LOGS_DIR" ] || fail "--phase-logs-dir is required"

# ---- Validate the structured record (no zeroed fields) ----------------------
case "$SCENARIO" in inserted-guard|flipped-comparison|silent-return) ;; *) fail "unknown scenario: $SCENARIO" ;; esac
case "$VARIANT" in vanilla|ripr-context|ripr-tool) ;; *) fail "unknown variant: $VARIANT" ;; esac
case "$DISCRIMINATES" in yes|no) ;; *) fail "--discriminates must be yes|no" ;; esac
# Match each complete scalar, not individual lines: line-oriented grep would
# accept an injected newline when any one line matches the expression.
[[ "$TEST_NAME" =~ ^[a-zA-Z_][a-zA-Z0-9_]*$ ]] \
  || fail "--test-name must be a Rust function identifier"
[[ "$RUN_ID" =~ ^${SCENARIO}-${VARIANT}-r[0-9]{2}$ ]] \
  || fail "run-id '$RUN_ID' must match <scenario>-<variant>-rNN"
[[ "$ROUNDS" =~ ^[1-9][0-9]*$ ]] \
  || fail "--rounds must be a positive integer (got '$ROUNDS')"
[ -f "$CONTEXT_FILE" ] || fail "context file not found: $CONTEXT_FILE"
[ -f "$EVAL_OUTPUT" ]  || fail "eval output file not found: $EVAL_OUTPUT"
[ -f "$FINAL_TEST" ]   || fail "final test file not found: $FINAL_TEST"
[ -s "$PHASE_LOGS_DIR/eval-original.log" ] || fail "original phase log missing or empty in $PHASE_LOGS_DIR"
[ -s "$PHASE_LOGS_DIR/eval-changed.log" ] || fail "changed phase log missing or empty in $PHASE_LOGS_DIR"

CONTEXT_BYTES="$(wc -c < "$CONTEXT_FILE" | tr -d ' ')"
[ "$CONTEXT_BYTES" -gt 0 ] || fail "measured context_bytes is 0 for $CONTEXT_FILE"

grep -qF "fn ${TEST_NAME}(" "$FINAL_TEST" \
  || fail "final test file does not define fn ${TEST_NAME}("

# The recorded verdict must equal the evaluator's own verdict line.
EV="$(tr -d '\r' < "$EVAL_OUTPUT" | grep -m1 '^DISCRIMINATES: ' || true)"
[ "$EV" = "DISCRIMINATES: $DISCRIMINATES" ] \
  || fail "--discriminates '$DISCRIMINATES' contradicts evaluator output ('$EV')"

# ---- Write the report atomically --------------------------------------------
mkdir -p "$RESULTS_DIR"
# Serialize the report and derived-summary writes. A stale lock is not removed
# automatically: the owner must first establish that no writer is still active.
LOCK="$RESULTS_DIR/.record-run.lock"
mkdir "$LOCK" 2>/dev/null || fail "another record writer holds $LOCK (or a stale lock needs inspection)"
REPORT="$RESULTS_DIR/$RUN_ID.md"
TMP_REPORT="$RESULTS_DIR/.${RUN_ID}.md.tmp$$"
TMP_CSV="$RESULTS_DIR/.summary.csv.tmp$$"
trap 'rm -f "$TMP_REPORT" "$TMP_CSV"; rmdir "$LOCK"' EXIT
[ ! -e "$REPORT" ] || fail "report already exists for run-id $RUN_ID; choose a new run-id"

{
  printf 'run_id: %s\n' "$RUN_ID"
  printf 'scenario: %s\n' "$SCENARIO"
  printf 'variant: %s\n' "$VARIANT"
  printf 'context_bytes: %s\n' "$CONTEXT_BYTES"
  printf 'rounds: %s\n' "$ROUNDS"
  printf 'test_name: %s\n' "$TEST_NAME"
  printf 'final_test_source: |\n'
  sed 's/^/  /' "$FINAL_TEST"
  printf 'eval_output: |\n'
  sed 's/^/  /' "$EVAL_OUTPUT"
  printf 'discriminates: %s\n' "$DISCRIMINATES"
  printf 'phase_original_log: |\n'
  sed 's/^/  /' "$PHASE_LOGS_DIR/eval-original.log"
  printf 'phase_changed_log: |\n'
  sed 's/^/  /' "$PHASE_LOGS_DIR/eval-changed.log"
  if [ -n "$NOTES" ]; then
    printf 'notes: |\n'
    printf '%s\n' "$NOTES" | sed 's/^/  /'
  fi
} > "$TMP_REPORT"
mv -f "$TMP_REPORT" "$REPORT"

# ---- Rebuild summary.csv from every report in the directory (atomic) --------
CSV_HEADER="run_id,scenario,variant,context_bytes,rounds,discriminates"
{ printf '%s\n' "$CSV_HEADER"
  for f in "$RESULTS_DIR"/*.md; do
    [ -f "$f" ] || continue
    # Only run records carry the run_id field; skip other markdown in the
    # directory (e.g. README.md) so a record write never fails on them.
    head -n 5 "$f" | grep -q '^run_id: ' || continue
    # The first five lines are the fixed header in both new and seed reports.
    # The verdict follows the indented evidence blocks, before notes. Stop at
    # notes so historical unindented note text cannot impersonate a field.
    id="$(sed -n '1s/^run_id: //p' "$f")"
    sc="$(sed -n '2s/^scenario: //p' "$f")"
    va="$(sed -n '3s/^variant: //p' "$f")"
    cb="$(sed -n '4s/^context_bytes: //p' "$f")"
    ro="$(sed -n '5s/^rounds: //p' "$f")"
    di="$(awk '/^notes:/{exit} /^discriminates: /{n++; value=substr($0,16)} END {if (n != 1) exit 1; print value}' "$f")" \
      || { echo "ERROR: report $f has an ambiguous verdict; refusing to write summary" >&2; exit 2; }
    case "$sc" in inserted-guard|flipped-comparison|silent-return) ;; *) fail "invalid scenario in $f" ;; esac
    case "$va" in vanilla|ripr-context|ripr-tool) ;; *) fail "invalid variant in $f" ;; esac
    case "$di" in yes|no) ;; *) fail "invalid verdict in $f" ;; esac
    [[ "$id" =~ ^${sc}-${va}-r[0-9]{2}$ ]] || fail "invalid run-id in $f"
    [ "$(basename "$f")" = "$id.md" ] || fail "run-id does not match report filename in $f"
    [[ "$cb" =~ ^[1-9][0-9]*$ ]] || fail "invalid context_bytes in $f"
    [[ "$ro" =~ ^[1-9][0-9]*$ ]] || fail "invalid rounds in $f"
    printf '%s,%s,%s,%s,%s,%s\n' "$id" "$sc" "$va" "$cb" "$ro" "$di"
  done
} > "$TMP_CSV"
mv -f "$TMP_CSV" "$RESULTS_DIR/summary.csv"

echo "RECORDED: $REPORT (context_bytes=$CONTEXT_BYTES, rounds=$ROUNDS, discriminates=$DISCRIMINATES)"
echo "SUMMARY: $RESULTS_DIR/summary.csv rebuilt from $(ls "$RESULTS_DIR"/*.md | wc -l | tr -d ' ') report(s)"
