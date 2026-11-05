#!/usr/bin/env bash
# make-scenario-crates.sh -- materialize the three agent-discrimination
# scenario crates as fresh two-commit git repositories.
#
# Usage: bash make-scenario-crates.sh <dest-root>
#
# Each scenario is wiped and rebuilt under <dest-root>/<scenario>/ as its own
# git repo with EXACTLY two commits so `--base HEAD~1` works:
#   commit 1 (ORIGINAL behavior) -> commit 2 (the CHANGE under study).
# Commit 2's tree is the crate's delivered content; the existing unit tests
# (#[cfg(test)] in src/lib.rs) and integration tests (tests/baseline.rs) pass
# on BOTH commits: they do not discriminate the change.
#
# Commits are deterministic: fixed identity (agent-bench <agent-bench@local>)
# and pinned author/committer timestamps reproduce the recorded seed-study
# SHAs byte for byte; the script fails closed on any mismatch.
#
# Validity (issue #13113): exactly one writer owns one materialized crate.
# Concurrent runs must each pass their own run-namespaced <dest-root>, and
# the destination must not be shared temp space (other runtimes have followed
# the same RUNBOOK from a shared temp directory before). The scripts enforce
# the same shared-temp refusal as eval.sh.
set -euo pipefail

BENCH_DIR="$(cd "$(dirname "$0")" && pwd)"

DEST="${1:-}"
[ -n "$DEST" ] || { echo "Usage: bash make-scenario-crates.sh <dest-root>" >&2; exit 2; }
mkdir -p "$DEST"
DEST="$(cd "$DEST" && pwd -W 2>/dev/null || pwd)"

# Refuse shared temp roots (Windows drive F:/Temp..., MSYS /f/temp/...,
# Unix /tmp..., /var/tmp...). See RUNBOOK.md "Shared-temp prohibition".
LOWER="$(printf '%s' "$DEST" | tr '[:upper:]' '[:lower:]')"
case "$LOWER" in
  ?:\/temp|?:\/temp/*|*/temp/*|/f/temp|/f/temp/*|/tmp|/tmp/*|/var/tmp|/var/tmp/*)
    echo "ERROR: refusing to materialize scenario crates into shared temp space: $DEST" >&2
    echo "       Use a run-namespaced directory outside shared temp (see RUNBOOK.md)." >&2
    exit 2 ;;
esac

# Conservatively refuse any ancestor Cargo manifest. A workspace ancestor can
# claim the nested package; some non-workspace manifests would be safe.
DIR_WALK="$DEST"
while :; do
  if [ -f "$DIR_WALK/Cargo.toml" ]; then
    echo "ERROR: refusing to materialize under '$DEST': ancestor '$DIR_WALK/Cargo.toml' is a Cargo manifest." >&2
    echo "       an ancestor workspace may claim the scenario package." >&2
    echo "       Materialize under a run-namespaced directory outside any ancestor Cargo manifest (see RUNBOOK.md)." >&2
    exit 2
  fi
  case "$DIR_WALK" in [A-Za-z]:|[A-Za-z]:/|/|.) break ;; esac
  PARENT="$(dirname "$DIR_WALK")"
  [ "$PARENT" = "$DIR_WALK" ] && break
  DIR_WALK="$PARENT"
done

IDENT_NAME=agent-bench
IDENT_EMAIL=agent-bench@local

commit_tree() { # $1=crate $2=date $3=message
  git -C "$1" add -A
  env GIT_AUTHOR_NAME="$IDENT_NAME" GIT_AUTHOR_EMAIL="$IDENT_EMAIL" GIT_AUTHOR_DATE="$2" \
      GIT_COMMITTER_NAME="$IDENT_NAME" GIT_COMMITTER_EMAIL="$IDENT_EMAIL" GIT_COMMITTER_DATE="$2" \
      git -C "$1" -c commit.gpgsign=false commit -q -m "$3"
}

materialize() { # $1=scenario $2=date1 $3=date2 $4=msg1 $5=msg2 $6=expected-head
  local scenario="$1" d1="$2" d2="$3" m1="$4" m2="$5" want="$6"
  local src="$BENCH_DIR/scenarios/$scenario"
  local crate="$DEST/$scenario"
  [ -d "$src/original" ] && [ -d "$src/changed" ] || {
    echo "ERROR: missing scenarios/$scenario/original or /changed trees" >&2; exit 1; }
  rm -rf "$crate"
  mkdir -p "$crate"
  cp -r "$src/original/." "$crate/"
  git -C "$crate" init -q -b main
  commit_tree "$crate" "$d1" "$m1"
  cp -r "$src/changed/." "$crate/"
  commit_tree "$crate" "$d2" "$m2"
  local got
  got="$(git -C "$crate" rev-parse HEAD)"
  if [ "$got" != "$want" ]; then
    echo "ERROR: $scenario HEAD mismatch: got $got, expected $want" >&2
    echo "       The materialized commits are not the recorded scenario identity; do not benchmark against them." >&2
    exit 1
  fi
  echo "$scenario: $got  \"$m2\""
}

# Scenario identity pinned to the recorded Nov 2-4 seed-study commits
# (contexts/ and results/seed-study/ were captured against these).
materialize inserted-guard \
  "2026-11-04T15:50:04-05:00" "2026-11-04T15:50:08-05:00" \
  "Initial pricing helpers with baseline tests" \
  "Return zero for negative amounts in total_after_fee" \
  "86625387aaf5908ada16a691fe11b2e1616f86ad"

materialize flipped-comparison \
  "2026-11-04T15:50:11-05:00" "2026-11-04T15:50:12-05:00" \
  "Initial membership pricing rules with baseline tests" \
  "Require age over 65 for the senior discount" \
  "006aaf48af8a8e8582056d85ad87d12e9690682d"

materialize silent-return \
  "2026-11-04T15:50:21-05:00" "2026-11-04T15:50:24-05:00" \
  "Initial settlement timing rules with baseline tests" \
  "Return early for cash-on-delivery captures" \
  "ed7af52a019f78987958eaf4f0190d99e0a2a8b0"

echo "scenario crates materialized OK under $DEST"
