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
#     include every valid report on successful rebuild (the seed-study summary
#     was missing 4 of 9 rows). A crash between renames can leave it stale.
#
# Usage:
#   bash record-run.sh --results-dir <dir> --run-id <scenario>-<variant>-rNN \
#     --scenario <scenario> --variant <vanilla|ripr-context|ripr-tool> \
#     --context-file <path-to-context-or-saved-ripr-output> \
#     --rounds <N> --test-name <fn> --discriminates <yes|no> \
#     --eval-output <file-with-verbatim-eval.sh-output> \
#     --final-test <path-to-tests/discriminate.rs> [--notes "<text>"]
set -euo pipefail

RESULTS_DIR="" RUN_ID="" SCENARIO="" VARIANT="" CONTEXT_FILE="" ROUNDS=""
TEST_NAME="" DISCRIMINATES="" EVAL_OUTPUT="" FINAL_TEST="" NOTES=""

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

# ---- Validate the structured record (no zeroed fields) ----------------------
case "$SCENARIO" in inserted-guard|flipped-comparison|silent-return) ;; *) fail "unknown scenario: $SCENARIO" ;; esac
case "$VARIANT" in vanilla|ripr-context|ripr-tool) ;; *) fail "unknown variant: $VARIANT" ;; esac
case "$DISCRIMINATES" in yes|no) ;; *) fail "--discriminates must be yes|no" ;; esac
printf '%s' "$RUN_ID" | grep -Eq "^${SCENARIO}-${VARIANT}-r[0-9]{2}$" \
  || fail "run-id '$RUN_ID' must match <scenario>-<variant>-rNN"
printf '%s' "$ROUNDS" | grep -Eq "^[1-9][0-9]*$" \
  || fail "--rounds must be a positive integer (got '$ROUNDS')"
[ -f "$CONTEXT_FILE" ] || fail "context file not found: $CONTEXT_FILE"
[ -f "$EVAL_OUTPUT" ]  || fail "eval output file not found: $EVAL_OUTPUT"
[ -f "$FINAL_TEST" ]   || fail "final test file not found: $FINAL_TEST"

CONTEXT_BYTES="$(wc -c < "$CONTEXT_FILE" | tr -d ' ')"
[ "$CONTEXT_BYTES" -gt 0 ] || fail "measured context_bytes is 0 for $CONTEXT_FILE"

grep -qF "fn ${TEST_NAME}(" "$FINAL_TEST" \
  || fail "final test file does not define fn ${TEST_NAME}("

# The recorded verdict must equal the evaluator's own verdict line.
EV="$(tr -d '\r' < "$EVAL_OUTPUT" | grep -m1 '^DISCRIMINATES: ' || true)"
[ "$EV" = "DISCRIMINATES: $DISCRIMINATES" ] \
  || fail "--discriminates '$DISCRIMINATES' contradicts evaluator output ('$EV')"

# Bind the recorded source bytes to the source used by the evaluator.
EVAL_HASH="$(tr -d '\r' < "$EVAL_OUTPUT" | sed -n 's/^TEST_SOURCE_SHA256: //p')"
SOURCE_HASH="$(sha256sum "$FINAL_TEST" | awk '{print $1}')"
[ "$EVAL_HASH" = "$SOURCE_HASH" ] || fail "final test source does not match evaluator source digest"

# ---- Write the report atomically --------------------------------------------
mkdir -p "$RESULTS_DIR"
# A summary is a snapshot of all reports. Fail closed when another writer is
# rebuilding it so an older snapshot cannot overwrite a newer one.
LOCK="$RESULTS_DIR/.record-run.lock"
mkdir "$LOCK" 2>/dev/null || fail "another record writer holds $LOCK (or a stale lock needs inspection)"
REPORT="$RESULTS_DIR/$RUN_ID.md"
TMP_REPORT="$RESULTS_DIR/.${RUN_ID}.md.tmp$$"
trap 'rm -f "$TMP_REPORT" "$TMP_CSV"; rmdir "$LOCK"' EXIT
TMP_CSV="$RESULTS_DIR/.summary.csv.tmp$$"

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
  if [ -n "$NOTES" ]; then
    printf 'notes: %s\n' "$NOTES"
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
    id="$(sed -n 's/^run_id: //p' "$f")"
    sc="$(sed -n 's/^scenario: //p' "$f")"
    va="$(sed -n 's/^variant: //p' "$f")"
    cb="$(sed -n 's/^context_bytes: //p' "$f")"
    ro="$(sed -n 's/^rounds: //p' "$f")"
    di="$(sed -n 's/^discriminates: //p' "$f")"
    if [ -z "$id" ] || [ -z "$sc" ] || [ -z "$va" ] || [ -z "$cb" ] || [ -z "$ro" ] || [ -z "$di" ]; then
      echo "ERROR: report $f has an empty structured field; refusing to write summary" >&2
      exit 2
    fi
    printf '%s,%s,%s,%s,%s,%s\n' "$id" "$sc" "$va" "$cb" "$ro" "$di"
  done
} > "$TMP_CSV"
mv -f "$TMP_CSV" "$RESULTS_DIR/summary.csv"

echo "RECORDED: $REPORT (context_bytes=$CONTEXT_BYTES, rounds=$ROUNDS, discriminates=$DISCRIMINATES)"
echo "SUMMARY: $RESULTS_DIR/summary.csv rebuilt from $(ls "$RESULTS_DIR"/*.md | wc -l | tr -d ' ') report(s)"
