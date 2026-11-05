#!/usr/bin/env bash
# build-contexts.sh -- regenerate the A/B harness context files for ONE run.
#
# Usage:
#   bash build-contexts.sh <crate-path> <ripr-bin> <out-dir> <scenario> <run-id>
#
# Writes <out-dir>/{vanilla,ripr-context,tool-note}.txt against the run's own
# materialized crate clone. All paths inside the generated files point at
# that clone and at <out-dir> -- never at a shared crate or shared temp space
# (per-run clone protocol, issue #13113). The recorded Nov 2-4 captures under
# contexts/ are the seed-study evidence; regenerate per run for new runs.
#
# Step zero normalizes the toolchain PATH (issue #13113): cargo and ripr must
# be resolvable, and $HOME/.cargo/bin is added when cargo is missing.
# ripr stderr of each call is kept in a sidecar .stderr.txt next to the
# capture.
set -euo pipefail

if ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not on PATH and $HOME/.cargo/bin/cargo not found"; exit 2; }

[ $# -eq 5 ] || { echo "Usage: bash build-contexts.sh <crate-path> <ripr-bin> <out-dir> <scenario> <run-id>" >&2; exit 2; }
CRATE="$1"; RIPR_BIN="$2"; OUT="$3"; SCENARIO="$4"; RUN_ID="$5"

[ -d "$CRATE" ] || { echo "ERROR: crate path not found: $CRATE" >&2; exit 2; }
[ -x "$RIPR_BIN" ] || { echo "ERROR: ripr binary not executable: $RIPR_BIN" >&2; exit 2; }
CRATE="$(cd "$CRATE" && pwd -W 2>/dev/null || pwd)"

LOWER="$(printf '%s' "$CRATE" | tr '[:upper:]' '[:lower:]')"
case "$LOWER" in
  ?:\/temp|?:\/temp/*|*/temp/*|/f/temp|/f/temp/*|/tmp|/tmp/*|/var/tmp|/var/tmp/*)
    echo "ERROR: refusing to build contexts for a crate in shared temp space: $CRATE" >&2
    exit 2 ;;
esac

# Match the generator and evaluator: reject a crate below a Cargo manifest
# before writing any context files that imply the run is usable.
DIR_WALK="$(dirname "$CRATE")"
while [ "$DIR_WALK" != / ] && [ "$DIR_WALK" != . ]; do
  if [ -f "$DIR_WALK/Cargo.toml" ]; then
    echo "ERROR: crate '$CRATE' sits under Cargo manifest '$DIR_WALK/Cargo.toml'; materialize outside Cargo workspaces." >&2
    exit 2
  fi
  PARENT="$(dirname "$DIR_WALK")"
  [ "$PARENT" = "$DIR_WALK" ] && break
  DIR_WALK="$PARENT"
done

git -C "$CRATE" rev-parse --verify -q HEAD~1 >/dev/null || { echo "ERROR: $CRATE has no HEAD~1 (need 2 commits)" >&2; exit 2; }
mkdir -p "$OUT"
OUT="$(cd "$OUT" && { pwd -W 2>/dev/null || pwd; })"

RIPR_VERSION="$("$RIPR_BIN" --version)"
echo "ripr: $RIPR_VERSION"

# ---- vanilla.txt ------------------------------------------------------------
{
  echo "# CONTEXT: vanilla"
  echo "# Crate: $SCENARIO"
  echo "# The complete change under study (git diff HEAD~1..HEAD),"
  echo "# followed by the full current source file."
  echo
  echo "===== git diff HEAD~1..HEAD ====="
  git -C "$CRATE" diff HEAD~1 HEAD
  echo
  echo "===== full changed source: src/lib.rs (current HEAD) ====="
  cat "$CRATE/src/lib.rs"
} > "$OUT/vanilla.txt"

# ---- ripr-context.txt -------------------------------------------------------
"$RIPR_BIN" check --root "$CRATE" --base HEAD~1 --format agent \
  > "$OUT/ripr-check-agent.txt" 2> "$OUT/ripr-check-agent.stderr.txt" \
  || echo "ripr check exited nonzero" >> "$OUT/ripr-check-agent.stderr.txt"

next_cmd="$(tr -d '\r' < "$OUT/ripr-check-agent.txt" | grep -m1 '^next=ripr ' | sed 's/^next=ripr //' || true)"
# Unquoted expansion below does not re-parse quoting, so the literal quotes
# ripr prints around "HEAD~1" must be stripped before invoking.
next_cmd="${next_cmd//\"/}"
if [ -n "$next_cmd" ]; then
  # Run exactly the follow-up command ripr nominates (substituting our
  # absolute binary path for the bare `ripr` it assumes on PATH).
  "$RIPR_BIN" $next_cmd > "$OUT/ripr-explain.txt" 2> "$OUT/ripr-explain.stderr.txt" \
    || echo "ripr explain exited nonzero" >> "$OUT/ripr-explain.stderr.txt"
  had_explain=yes
else
  echo "no next= line in check output; explain not captured" > "$OUT/ripr-explain.txt"
  had_explain=no
fi

{
  echo "# CONTEXT: ripr analyzer output"
  echo "# Crate: $SCENARIO"
  echo "# ripr version: $RIPR_VERSION"
  echo "# Captured with: ripr check --root <CRATE_PATH> --base HEAD~1 --format agent"
  echo "# plus ripr explain on the top finding (ripr's own suggested follow-up)."
  echo "# ripr is a static advisory tool: it does not write tests."
  echo
  echo "===== ripr check --root <CRATE_PATH> --base HEAD~1 --format agent ====="
  cat "$OUT/ripr-check-agent.txt"
  if [ "$had_explain" = "yes" ]; then
    echo
    echo "===== ripr explain (top finding): $next_cmd ====="
    cat "$OUT/ripr-explain.txt"
  fi
} > "$OUT/ripr-context.txt"

# ---- tool-note.txt ----------------------------------------------------------
cat > "$OUT/tool-note.txt" <<EOF
# TOOL NOTE: the ripr analyzer

ripr ($RIPR_VERSION) is available at: $RIPR_BIN
Run it from any shell. Verified invocations for this crate:

  $RIPR_BIN check --root $CRATE --base HEAD~1 --format agent

That prints a compact agent-format advisory for the changed behavior between
HEAD~1 and HEAD. Each finding line ends with id=... . The output also contains
a 'next=ripr ...' line nominating the exact follow-up command for the top
finding. Run it by substituting the absolute binary path for 'ripr', e.g.:

  $RIPR_BIN explain --root $CRATE --base "HEAD~1" --committed <FINDING_ID>

ripr is a static advisory analyzer: it reports exposure evidence and draft
test intent (classifications such as exposed / weakly_exposed /
reachable_unrevealed). It does not write or run tests, and it never promotes
static evidence to killed/survived/proven.

If you run ripr, save the output you used to:
  $OUT/ripr-run-output-$RUN_ID.txt
EOF

echo "    vanilla.txt      $(wc -c < "$OUT/vanilla.txt") bytes"
echo "    ripr-context.txt $(wc -c < "$OUT/ripr-context.txt") bytes"
echo "    tool-note.txt    $(wc -c < "$OUT/tool-note.txt") bytes (explain captured: $had_explain)"
echo "contexts built for $SCENARIO run $RUN_ID in $OUT"
