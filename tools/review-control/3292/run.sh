#!/usr/bin/env bash
set -euo pipefail

BASE_SHA=261aa86514ee1ae273ac65f7c6351e47cd50f47f
TARGET_BRANCH=fix/3292-source-currentness-receipts

control_root=$(git rev-parse --show-toplevel)
repair_dir="$RUNNER_TEMP/ripr-3292-repair"
receipt_dir="$control_root/target/review-control/3292"
receipt="$receipt_dir/receipt.json"
rm -rf "$repair_dir" "$receipt_dir"
mkdir -p "$receipt_dir"

git fetch --no-tags origin main
test "$(git rev-parse origin/main)" = "$BASE_SHA" || {
  echo "main moved from reviewed #3291 merge $BASE_SHA; refusing to build a stale follow-up" >&2
  exit 1
}
test -z "$(git ls-remote --heads origin "refs/heads/$TARGET_BRANCH")" || {
  echo "target branch already exists: $TARGET_BRANCH" >&2
  exit 1
}

git worktree add --detach "$repair_dir" "$BASE_SHA"
git -C "$repair_dir" config user.name EffortlessSteven
git -C "$repair_dir" config user.email git@effortlesssteven.com

python "$control_root/tools/review-control/3292/repair.py" "$repair_dir" "$receipt"

git -C "$repair_dir" diff --check
cargo fmt --manifest-path "$repair_dir/Cargo.toml" --all -- --check
(
  cd "$repair_dir"
  cargo test --locked -p ripr source_currentness -- --nocapture
  cargo test --locked -p ripr proptest_render_preserves_top_level_text_and_emits_valid_json -- --nocapture
  cargo run --locked -p xtask -- goldens check
  cargo run --locked -p xtask -- check-output-contracts
  cargo run --locked -p xtask -- check-spec-format
  cargo run --locked -p xtask -- check-traceability
  cargo run --locked -p xtask -- check-doc-artifacts
)

mapfile -t changed < <(git -C "$repair_dir" diff --name-only)
expected_non_fixture=(
  "crates/ripr/src/domain/probe.rs"
  "docs/specs/RIPR-SPEC-0151-source-currentness.md"
  "pr-tmp/pr-3280-body.md"
)
for path in "${expected_non_fixture[@]}"; do
  printf '%s\n' "${changed[@]}" | grep -Fxq "$path" || {
    echo "expected repair path missing: $path" >&2
    exit 1
  }
done
for path in "${changed[@]}"; do
  case "$path" in
    crates/ripr/src/domain/probe.rs|docs/specs/RIPR-SPEC-0151-source-currentness.md|pr-tmp/pr-3280-body.md|fixtures/*/expected/CHANGELOG.md)
      ;;
    *)
      echo "unexpected repair path: $path" >&2
      exit 1
      ;;
  esac
done

python - "$receipt" "${#changed[@]}" <<'PY'
import json
from pathlib import Path
import sys
path = Path(sys.argv[1])
value = json.loads(path.read_text(encoding="utf-8"))
value["changed_path_count"] = int(sys.argv[2])
value["verification"] = [
    "git diff --check",
    "cargo fmt --all -- --check",
    "cargo test -p ripr source_currentness",
    "cargo test -p ripr proptest_render_preserves_top_level_text_and_emits_valid_json",
    "cargo xtask goldens check",
    "cargo xtask check-output-contracts",
    "cargo xtask check-spec-format",
    "cargo xtask check-traceability",
    "cargo xtask check-doc-artifacts",
]
path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
PY

git -C "$repair_dir" add -A
git -C "$repair_dir" commit -m "fix(review): reconcile source-currentness receipts (#3292)"
head=$(git -C "$repair_dir" rev-parse HEAD)
git push origin "$head:refs/heads/$TARGET_BRANCH"

printf '%s\n' \
  "BASE_SHA=$BASE_SHA" \
  "TARGET_BRANCH=$TARGET_BRANCH" \
  "HEAD_SHA=$head" \
  >> "$receipt_dir/identities.env"
git -C "$repair_dir" diff-tree --no-commit-id --name-status -r "$head" > "$receipt_dir/name-status.txt"
git worktree remove --force "$repair_dir"
