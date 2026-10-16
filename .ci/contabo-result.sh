#!/usr/bin/env bash
set -euo pipefail
# Central immutable import owns native authentication, bounded membership,
# sanitization and command/source verification. This policy consumes that import;
# it never rebuilds the product or accepts a producer workspace as an import.
[[ "${EM_CI_LANE:-}" == ripr-product ]]
[[ "${EM_CI_SELECTED_PROOF_RESULT:-}" == success ]]
[[ "${GITHUB_REPOSITORY:-}" == EffortlessMetrics/ripr-swarm ]]
[[ "${GITHUB_EVENT_NAME:-}" == workflow_dispatch ]]
[[ -n "${EM_CI_ARTIFACT_DIR:-}" && -d "$EM_CI_ARTIFACT_DIR" && ! -L "$EM_CI_ARTIFACT_DIR" ]]
context="$EM_CI_ARTIFACT_DIR/em-ci-ripr-product-context.json"
[[ -f "$context" && ! -L "$context" ]]
base=$(jq -er --arg subject "$GITHUB_SHA" '
  .inputs | select(.expected_sha == $subject) | .base_sha
  | select(type == "string" and test("^[0-9a-f]{40}$") and . != $subject)
' "$GITHUB_EVENT_PATH")
tree=$(git rev-parse 'HEAD^{tree}')
[[ "$(git rev-parse HEAD)" == "$GITHUB_SHA" ]]
jq -e --arg repository "$GITHUB_REPOSITORY" --arg sha "$GITHUB_SHA" \
  --arg run "$GITHUB_RUN_ID" --arg attempt "$GITHUB_RUN_ATTEMPT" '
  .schema == "em-ci-rust-result.v1" and .repository == $repository and
  .sha == $sha and .run_id == $run and .run_attempt == $attempt and
  .profile == "standard" and .route == "self_hosted" and
  .result == "success" and .proof_outcome == "success"
' <<< "$EM_CI_EVIDENCE" > /dev/null
jq -e --arg repository "$GITHUB_REPOSITORY" --arg sha "$GITHUB_SHA" \
  --arg base "$base" --arg tree "$tree" --arg run "$GITHUB_RUN_ID" \
  --arg attempt "$GITHUB_RUN_ATTEMPT" '
  .schema == "em-ci-ripr-product-context/v1" and
  .identity.repository == $repository and .identity.subject == $sha and
  .identity.base == $base and .identity.tree == $tree and
  .identity.run == $run and .identity.attempt == $attempt and
  .identity.defining_workflow_sha == "8ff77e5876b4a7808a58e053aa6248b1020e02d0" and
  (.identity.producer_check_run_id | type == "number" and . > 0 and floor == .) and
  .native_outcome == "success" and .retention_days == 14 and
  .selection == "source-derived-v1" and
  .verdict == {"disposition":"product_pass","exit":0}
' "$context" > /dev/null
printf '%s\n' 'RIPR product result policy accepted authenticated exact-subject import.'
