#!/usr/bin/env bash
set -euo pipefail

BASE_SHA=d0439714e1180ad920480917a6b47dbeff99c7a1
TARGET_BRANCH=fix/3281-candidate-actionability
PATCH_SHA256=483fce86dfc7e6abb7c020ce81edca3098ae8a3958a2f68a35f1b5ff05cc9c0e
PATCH_GZIP_SHA256=3f4e8d1d72954fe4b486c468be9d5c2646a4fa52f81eed8c8a133e5ba64d47a6

control_root=$(git rev-parse --show-toplevel)
candidate_dir="$RUNNER_TEMP/ripr-3281-candidate"
packet="$control_root/target/review-control/3281"
payload="$control_root/tools/review-control/3281/candidate.patch.gz.b64"
new_files="$control_root/tools/review-control/3281/files"
patch_gz="$RUNNER_TEMP/ripr-3281-candidate.patch.gz"
patch="$RUNNER_TEMP/ripr-3281-candidate.patch"
status_file="$packet/status.env"
rm -rf "$candidate_dir" "$packet" "$patch_gz" "$patch"
mkdir -p "$packet"

finalize() {
  rc=$?
  set +e
  if test -d "$candidate_dir"; then
    git -C "$candidate_dir" status --short > "$packet/status.txt" 2>&1 || true
    git -C "$candidate_dir" diff --cached --binary "$BASE_SHA" > "$packet/candidate.patch" 2>/dev/null || true
    git -C "$candidate_dir" diff --cached --stat "$BASE_SHA" > "$packet/candidate.stat" 2>/dev/null || true
    git -C "$candidate_dir" diff --cached --name-status "$BASE_SHA" > "$packet/name-status.txt" 2>/dev/null || true
  fi
  printf '%s\n' \
    "BASE_SHA=$BASE_SHA" \
    "TARGET_BRANCH=$TARGET_BRANCH" \
    "EXIT_CODE=$rc" \
    "PATCH_SHA256=$PATCH_SHA256" \
    "PATCH_GZIP_SHA256=$PATCH_GZIP_SHA256" \
    > "$status_file"
  exit "$rc"
}
trap finalize EXIT

git fetch --no-tags origin main "$BASE_SHA"
git merge-base --is-ancestor "$BASE_SHA" origin/main || {
  echo "reviewed C2 base is no longer an ancestor of main" >&2
  exit 1
}
test -z "$(git ls-remote --heads origin "refs/heads/$TARGET_BRANCH")" || {
  echo "target branch already exists: $TARGET_BRANCH" >&2
  exit 1
}

git worktree add --detach "$candidate_dir" "$BASE_SHA"
git -C "$candidate_dir" config user.name EffortlessSteven
git -C "$candidate_dir" config user.email git@effortlesssteven.com

base64 --decode "$payload" > "$patch_gz"
test "$(sha256sum "$patch_gz" | awk '{print $1}')" = "$PATCH_GZIP_SHA256"
gzip --decompress --stdout "$patch_gz" > "$patch"
test "$(sha256sum "$patch" | awk '{print $1}')" = "$PATCH_SHA256"
git -C "$candidate_dir" apply --check "$patch"
git -C "$candidate_dir" apply "$patch"

install -D -m 0644 \
  "$new_files/crates/ripr/src/output/candidate_actionability.rs" \
  "$candidate_dir/crates/ripr/src/output/candidate_actionability.rs"
install -D -m 0644 \
  "$new_files/docs/specs/RIPR-SPEC-0152-candidate-actionability.md" \
  "$candidate_dir/docs/specs/RIPR-SPEC-0152-candidate-actionability.md"

cd "$candidate_dir"
cargo fmt --all
git add -A
git diff --cached --check

cargo test --locked -p ripr candidate_actionability -- --nocapture
cargo test --locked -p ripr source_currentness -- --nocapture
cargo test --locked -p ripr non_candidate -- --nocapture
cargo test --locked -p ripr pr_evidence -- --nocapture
cargo test --locked -p xtask pr_evidence -- --nocapture
cargo run --locked -p xtask -- check-output-contracts
cargo run --locked -p xtask -- check-spec-format
cargo run --locked -p xtask -- check-traceability
cargo run --locked -p xtask -- check-doc-artifacts
cargo run --locked -p xtask -- goldens check

changed_count=$(git diff --cached --name-only "$BASE_SHA" | wc -l | tr -d ' ')
cat > "$packet/receipt.json" <<JSON
{
  "schema": "ripr.candidate_actionability_builder.v1",
  "issue": 3281,
  "source_base": "$BASE_SHA",
  "target_branch": "$TARGET_BRANCH",
  "changed_path_count": $changed_count,
  "patch_sha256": "$PATCH_SHA256",
  "patch_gzip_sha256": "$PATCH_GZIP_SHA256",
  "verification": [
    "cargo fmt --all",
    "git diff --cached --check",
    "cargo test -p ripr candidate_actionability",
    "cargo test -p ripr source_currentness",
    "cargo test -p ripr non_candidate",
    "cargo test -p ripr pr_evidence",
    "cargo test -p xtask pr_evidence",
    "cargo xtask check-output-contracts",
    "cargo xtask check-spec-format",
    "cargo xtask check-traceability",
    "cargo xtask check-doc-artifacts",
    "cargo xtask goldens check"
  ],
  "claim_boundary": [
    "one producer-owned source-currentness projection controls candidate edit eligibility",
    "non-current evidence may remain visible but cannot authorize a current repair, gate, annotation, packet, mutation route, or editor action",
    "existing class, policy, suppression, route, packet-validation, and verification rules remain additional requirements"
  ],
  "non_claims": [
    "no source-currentness inference change",
    "no classification threshold change",
    "no release repin",
    "no downstream C3 containment removal"
  ]
}
JSON

git commit -m "feat(output): converge candidate actionability (#3281)"
head=$(git rev-parse HEAD)
git push origin "$head:refs/heads/$TARGET_BRANCH"
printf '%s\n' "HEAD_SHA=$head" >> "$status_file"
