# Live-history release transaction runbook

Status: active operator runbook for the `0.11.0` live-head transaction and
future releases using the same two-repository history model.

This document is the canonical lifecycle from the release boundary through
reopened swarm development. It sequences existing verifier contracts; it does
not replace their semantics or make publication implicit.

## Canonical `#2379` public → swarm → public order

For `0.11.0`, `#2379` establishes the **public → swarm → public** release order
(detailed per-stage ownership lives in
[the 0.11.0 cut checklist](releases/0.11.0-cut-checklist.md#02-phase-vocabulary)):

1. **Inbound public → swarm integration** (`ripr#1627` →
   `ripr#1768`/`#1769`/`#1770`, reconciled by `ripr#1762`): admit an executable
   history-preserving route, preserve public `P` in `ripr-swarm`, and land
   integration repairs before final swarm convergence. The older sequence that
   postponed the entire source sync until after the swarm cut is historical.
2. **Pre-cut product convergence, freeze, audit/denominator, pin, and
   qualification** in `ripr-swarm` (sections 1–3 below): finish selected
   product work (`#2766`), freeze the combined cut (`#5114`/`#5391`), complete
   pre-pin evidence (`#3807` → `#2766`/`#2768`), push the immutable swarm pin
   (`#1609`), and qualify that exact candidate (`#2769`).
3. **Qualification admission and guarded public return** (section 4 below):
   `ripr#1771` admits the exact qualification and `ripr#1772`/`#1773` construct
   and merge the guarded return to `EffortlessMetrics/ripr`.
4. **Final source release tail and ship packet** (sections 5–6 below):
   `ripr#1466`/`#1467` → `#1578`/`#1468`/`#1469` establish the integrated
   source artifacts and immutable ship-or-hold packet.
5. **Public RC and stable publication** (section 6 below):
   `ripr#1632`–`#1637` govern the exact authorized GitHub-only RC, independent
   verification/cohort, and `stable_ready_same_candidate` decision; separate
   per-channel `#1470` authorization governs stable publication
   (`ripr#1479`/`#1480`) and independent public verification (`ripr#1481`).
6. **Post-release back-sync `K` and rolling restoration** (sections 7–8 below):
   `#3419` → `#3313` → `#3420` run only after terminal `#1481` public
   verification.

## Authority and command labels

`EffortlessMetrics/ripr-swarm` is development authority and preserves every
commit reachable from the selected swarm main, including the reviewable history
of ordinary squash-merged feature/fix PRs.
`EffortlessMetrics/ripr` is source, release, and distribution authority.
Public tags, GitHub Releases, crates.io, VS Code Marketplace, and Open VSX are
source-side surfaces. Never publish or create a public-release tag from swarm.

Every command is marked:

- **[READ-ONLY]** — reads a checkout, receipt, GitHub state, or public channel.
- **[LOCAL-MUTATING]** — changes only a named local checkout, worktree, index,
  branch, or receipt file.
- **[EXTERNAL-PUBLISHING]** — changes a remote ref, policy, tag, release,
  registry, marketplace, or other external state. Public artifact, tag,
  release, and store operations require explicit channel authorization in
  source issue [#1470](https://github.com/EffortlessMetrics/ripr/issues/1470);
  control-plane ref and policy operations require owner approval and a receipt.

Examples use POSIX shell syntax. On Windows, translate syntax but preserve
exact SHAs and expected-head checks; never substitute floating `main`.
Every executable Bash block below begins in the same fail-fast transaction
shell: run `set -euo pipefail` before the block (and again when copying a block
into a new shell). A block copied without that session preamble is not valid
proof; an unset required variable or failed pipeline must stop the transaction.

## Packet, authority reset, and stop points

Create `target/ripr/release-transaction/<VERSION>/` outside tracked source.
The packet must bind `SOURCE_PARENT`, `SWARM_PARENT`, protected
`refs/tags/ripr-release-<VERSION>-<SWARM_PARENT>`, local verifier
`refs/ripr/release-<VERSION>-<SWARM_PARENT>`, `MERGE_BASE`, reviewed `JOIN_TREE`,
`J`, `SWARM_BEFORE`, `SOURCE_RELEASE_HEAD`, reviewed `BACK_SYNC_TREE`, and `K`.
Retain all-reachable and first-parent counts/digests, open-PR dispositions,
toolchain/version, routed-CI URL and `headSha`, conflict/resolution manifest,
artifact receipts, authorization, channel results, policy-before/exception/
after snapshots, and cleanup. JSON and Markdown projections must share inputs.
Run the command blocks in one operator shell, or export the variables before
continuing. The paths below are packet-local and are not repository authority.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=operator checkout; initialize transaction packet paths
VERSION=0.11.0
PACKET_ROOT="$(pwd)/target/ripr/release-transaction/${VERSION}"
mkdir -p "$PACKET_ROOT"
RELEASE_NOTES="$PACKET_ROOT/release-notes.md"
PUBLICATION_RECEIPT="$PACKET_ROOT/publication-receipt.json"
POLICY_BEFORE="$PACKET_ROOT/policy-before.json"
POLICY_EXCEPTION="$PACKET_ROOT/policy-temporary-exception.json"
POLICY_EXCEPTION_REQUEST="$PACKET_ROOT/policy-temporary-exception-request.json"
POLICY_RESTORE_REQUEST="$PACKET_ROOT/policy-restore-request.json"
POLICY_AFTER="$PACKET_ROOT/policy-after.json"
POLICY_APPROVAL="$PACKET_ROOT/policy-owner-approval.json"

# [READ-ONLY] repo=operator shell; require caller-selected fresh roots
SOURCE_ROOT="${SOURCE_ROOT:?set to the absolute path of a fresh ripr checkout}"
SWARM_ROOT="${SWARM_ROOT:?set to the absolute path of a fresh ripr-swarm checkout}"
test "$(git -C "$SOURCE_ROOT" rev-parse --is-inside-work-tree)" = true
test "$(git -C "$SWARM_ROOT" rev-parse --is-inside-work-tree)" = true
RUN_WAIT_SECONDS=1800
wait_for_run_success() {
  run_id="$1"
  deadline=$(( $(date +%s) + RUN_WAIT_SECONDS ))
  while true; do
    run_state="$(gh run view "$run_id" --repo EffortlessMetrics/ripr --json status,conclusion --jq '[.status, (.conclusion // "")] | @tsv')"
    status="${run_state%%$'\t'*}"
    conclusion="${run_state#*$'\t'}"
    if test "$status" = completed; then
      test "$conclusion" = success
      return 0
    fi
    test "$(date +%s)" -lt "$deadline" || { echo "workflow $run_id did not complete within $RUN_WAIT_SECONDS seconds" >&2; return 1; }
    sleep 15
  done
}
bind_new_dispatch_run() {
  before_file="$1"
  after_file="$2"
  sha="$3"
  ref="$4"
  since="$5"
  endpoint="$6"
  deadline=$(( $(date +%s) + 300 ))
  while test "$(date +%s)" -lt "$deadline"; do
    gh api --paginate --slurp "$endpoint" | jq '[.[] | .workflow_runs[]]' > "$after_file"
    jq -n --slurpfile before "$before_file" --slurpfile after "$after_file" \
      --arg sha "$sha" --arg ref "$ref" --arg since "$since" '
    ($before[0] | map(.id)) as $old_ids |
    [$after[0][] | .id as $run_id | select(.head_sha == $sha and .head_branch == $ref and
      .event == "workflow_dispatch" and .created_at >= $since and
      (($old_ids | index($run_id)) | not))] | unique_by(.id)' > "${after_file%.json}-new.json"
    count="$(jq 'length' "${after_file%.json}-new.json")"
    test "$count" -le 1 || return 1
    test "$count" = 1 && { jq -r '.[0].id' "${after_file%.json}-new.json"; return 0; }
    sleep 5
  done
  return 1
}
```

Historical candidate-only `C -> T`, hard-cut, replacement-freeze, and
candidate-ref receipts remain audit evidence but are superseded. They are not
the active `0.11.0` authority, denominator, pin, source parent, or permission.

Retained artifacts under `docs/release-candidates/` retain their #3842
lifecycle metadata and `cargo xtask check-release-targets` policy validation.
Changing a template's schema updates its own raw digest and human projection;
it does not activate a pin or rewrite historical receipt bytes. The broad
#3924 consumer-integration work remains deferred for 0.11.x. Direct #1609
schema-1.1 admission below binds the independently reviewed manifest and exact
owner packets without treating milestone policy as live-head selection.
Historical registry mode stays separately available and never becomes a
fallback from refused direct inputs.

Stop at once when a repository identity, owner, version, exact input, receipt,
expected head, policy state, or publication authorization is ambiguous. Green
CI, a ship packet, J, or K is evidence only; none authorizes publication.

## Operator discipline during the transaction

The transaction reuses this repository's standing operator contracts. They are
restated here only as pointers; their semantics stay with their owners:

- Any repair PR created during the transaction, and every queued PR
  dispositioned in section 1, converges through the ordinary PR qualification
  route: exactly one required context, earned by a fresh `merge-ready` label
  event on the exact head, never by an in-place rerun; resolve conflicts
  before re-firing the label. See
  [the PR convergence runbook](agent-context/convergence-runbook.md).
- Several autonomous actors may converge on the same transaction PR. Record
  one `Converging this PR: <actor id>` claim comment and respect an existing
  actor's claim ([PR automation](PR_AUTOMATION.md#multi-actor-ownership-signaling));
  the queue disposition above never closes another owner's live work to make
  the queue look empty.
- The section 3 qualification worktree may run on any host, but golden
  expected-files are arbitrated on Linux. Never bless or hand-edit golden
  files from a Windows host; route a re-bless through the WSL Ubuntu pinned
  toolchain
  ([validation](agent-context/validation.md#golden-fixtures-and-the-blessing-platform)).
- Between stages, main health is monitored by the scheduled main-health
  sentinel, not by the cancelled main push runs. Its `main-red` tracker issue
  is advisory evidence to reconcile before relying on unpinned main state;
  the sentinel is a probe and holds no branch-protection or ruleset authority
  ([CI](CI.md#main-health-sentinel)).

## 1. Reset truth and pre-cut queue observations

Confirm that inbound public → swarm integration (`ripr#1627` →
`ripr#1768`/`#1769`/`#1770`, reconciled by `ripr#1762`) has already landed in
`ripr-swarm` before finalizing the pre-cut denominator and advancing to
section 2 (`W` / `#1609`). If public `P` advances before the guarded return in
section 4, import the new public head into `ripr-swarm`, review the effects,
and renew affected proof before pinning or returning.

Use fresh checkouts. Ordinary source-repository development PRs are drift:
redirect them to swarm or classify them as release/security work before the
source hold. Record each remaining PR as included, deferred, superseded, or
not release-relevant; do not close another owner's work to make the queue look
empty.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=both fresh operator checkouts; fetch and reconcile live state
git -C "$SWARM_ROOT" fetch origin --prune --tags
git -C "$SOURCE_ROOT" fetch origin --prune --tags
git -C "$SWARM_ROOT" status --short --branch
git -C "$SOURCE_ROOT" status --short --branch
gh pr list --repo EffortlessMetrics/ripr-swarm --state open --limit 100
gh pr list --repo EffortlessMetrics/ripr --state open --limit 100
SOURCE_ORIGIN="$(git -C "$SOURCE_ROOT" remote get-url origin)"
SWARM_ORIGIN="$(git -C "$SWARM_ROOT" remote get-url origin)"
case "$SOURCE_ORIGIN" in
  git@github.com:EffortlessMetrics/ripr.git|https://github.com/EffortlessMetrics/ripr|https://github.com/EffortlessMetrics/ripr.git) ;;
  *) echo "unexpected source origin: $SOURCE_ORIGIN" >&2; exit 1 ;;
esac
case "$SWARM_ORIGIN" in
  git@github.com:EffortlessMetrics/ripr-swarm.git|https://github.com/EffortlessMetrics/ripr-swarm|https://github.com/EffortlessMetrics/ripr-swarm.git) ;;
  *) echo "unexpected swarm origin: $SWARM_ORIGIN" >&2; exit 1 ;;
esac
```

Admit only bounded final swarm cleanup: a release-blocking candidate failure,
source-preflight survivor, or required policy/evidence contract. After pinning,
later swarm-main merges are outside this release; branches may continue, but
their merges belong to the next release.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; capture exact heads and pin files
test -z "$(git -C "$SOURCE_ROOT" status --short)"
test -z "$(git -C "$SWARM_ROOT" status --short)"
SOURCE_OBSERVED_HEAD="$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)"
SWARM_PARENT="$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)"
test "$(git -C "$SOURCE_ROOT" rev-parse "$SOURCE_OBSERVED_HEAD^{commit}")" = "$SOURCE_OBSERVED_HEAD"
test "$(git -C "$SWARM_ROOT" rev-parse "$SWARM_PARENT^{commit}")" = "$SWARM_PARENT"
printf '%s\n' "$SOURCE_OBSERVED_HEAD" > "$PACKET_ROOT/source-observed-head"
printf '%s\n' "$SWARM_PARENT" > "$PACKET_ROOT/SWARM_PARENT"
```

```bash
set -euo pipefail
# [READ-ONLY] repo=packet+source+swarm; load pins and reject drift
SOURCE_OBSERVED_HEAD="$(tr -d '\r\n' < "$PACKET_ROOT/source-observed-head")"
SWARM_PARENT="$(tr -d '\r\n' < "$PACKET_ROOT/SWARM_PARENT")"
# SOURCE_OBSERVED_HEAD is observation only; public-return admission (ripr#1771 under ripr#1627/#1889) binds SOURCE_PARENT after swarm qualification.
test "$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)" = "$SWARM_PARENT"
```

The queue snapshot is produced by the two `gh pr list` calls above and bound
to the exact observed heads. Source observation is not a held SOURCE_PARENT.
Keep the producer output, not a hand-edited table:

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet+swarm; copy and validate the checked-in selection template before W
LIVE_HEAD_TEMPLATE="docs/release-candidates/0.11.0-live-head-selection.json"
cp "$LIVE_HEAD_TEMPLATE" "$PACKET_ROOT/live-head-selection-template.json"
jq -e '.schema_version == "1.1" and .status == "active_selection_template" and .candidate == null and .source_parent == null' "$LIVE_HEAD_TEMPLATE" >/dev/null
TEMPLATE_SHA256="$(sha256sum "$LIVE_HEAD_TEMPLATE" | awk '{print $1}')"
cargo xtask check-release-targets
jq -e --arg path "$LIVE_HEAD_TEMPLATE" --arg sha "$TEMPLATE_SHA256" '[.artifacts[] | select(.path == $path and .sha256 == $sha and .state == "active_selection_template" and .projection_of == null)] | length == 1' docs/release-candidates/index.json >/dev/null
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; retain observations, not candidate admission
gh pr list --repo EffortlessMetrics/ripr-swarm --state open --limit 100 --json number,title,headRefName,headRefOid,baseRefName,isDraft,updatedAt > "$PACKET_ROOT/swarm-open-prs.json"
gh pr list --repo EffortlessMetrics/ripr --state open --limit 100 --json number,title,headRefName,headRefOid,baseRefName,isDraft,updatedAt > "$PACKET_ROOT/source-open-prs.json"
jq -n --arg observed_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg source_observed_head "$SOURCE_OBSERVED_HEAD" --arg swarm_parent "$SWARM_PARENT" --slurpfile swarm "$PACKET_ROOT/swarm-open-prs.json" --slurpfile source "$PACKET_ROOT/source-open-prs.json" '{schema_version: 1, kind: "precut_queue_observation", observed_at_utc: $observed_at, source_observed_head: $source_observed_head, swarm_parent: $swarm_parent, queues: {swarm: $swarm[0], source: $source[0]}, claim: "observation only; not a candidate manifest or source hold"}' > "$PACKET_ROOT/precut-queue-observation.json"
```

Before W, inbound public → swarm integration (`ripr#1627` →
`ripr#1768`/`#1769`/`#1770`, tracked in `ripr-swarm#13325`) must already be
merged into `ripr-swarm`, and swarm movement extends the denominator and
requires a new current #2766/#2768/#3807 handoff. Source observations may
change during this stage; public-return admission (`ripr#1771` under
`ripr#1627`/`#1889`) re-reads public `main`, confirms public `P` is still
current and an ancestor of `SWARM_PARENT`, and binds `SOURCE_PARENT` only
after swarm qualification (`#2769`). After that source hold, movement
invalidates the corresponding source transaction. Never update only a SHA in
an existing immutable receipt.

## 2. Guarded swarm pin

`W` is the first successful push of the protected candidate tag below. The
checked-in schema-1.1 template is copied, not activated in place. Before W,
#2766 selected claims, #2768 denominator and #3807 audit must have current
accepted owner packets. The final manifest binds their raw bytes and actual
candidate/package identities. The template and a producer-emitted sidecar do
not establish those approvals. After W, unrelated main movement is outside the
candidate, never a silent membership update.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=swarm root; create only the local transaction ref
VERSION=0.11.0
PIN_TAG="ripr-release-${VERSION}-${SWARM_PARENT}"
SWARM_REF="refs/tags/$PIN_TAG"
VERIFIER_REF="refs/ripr/release-${VERSION}-${SWARM_PARENT}"
git -C "$SWARM_ROOT" update-ref "$VERIFIER_REF" "$SWARM_PARENT"
test "$(git -C "$SWARM_ROOT" rev-parse "$VERIFIER_REF^{commit}")" = "$SWARM_PARENT"
```

```bash
set -euo pipefail
# [READ-ONLY] repo=swarm remote; require protected candidate-tag guarantee
PIN_RULESET_ID="$(gh api repos/EffortlessMetrics/ripr-swarm/rulesets --paginate --jq '.[] | select(.name == "release-transaction-pins" and .target == "tag" and .enforcement == "active") | .id')"
test "$(printf '%s\n' "$PIN_RULESET_ID" | awk 'NF {count++} END {print count + 0}')" -eq 1
PIN_RULESET="$PACKET_ROOT/pin-ruleset.json"
gh api "repos/EffortlessMetrics/ripr-swarm/rulesets/${PIN_RULESET_ID}" > "$PIN_RULESET"
# GitHub's REST ruleset examples use fully qualified refs (for example,
# `refs/heads/main`), and its rule-suite API documents `refs/tags/` as the
# tag-ref prefix. See https://docs.github.com/en/rest/orgs/rules and
# https://docs.github.com/en/rest/repos/rule-suites. Keep the candidate tag
# name below unqualified (`ripr-release-...`), but require this repository's
# fail-closed policy contract to cover only the tag namespace. The checked-in
# fixture/test proves that local contract; it does not prove GitHub server
# acceptance or rejection. Verify live settings through the read-only receipt.
jq -e --arg tag "refs/tags/ripr-release-*" '(.target == "tag" and .enforcement == "active") and (.conditions.ref_name.include == [$tag]) and (any(.rules[]?; .type == "update")) and (any(.rules[]?; .type == "deletion")) and (.conditions.ref_name.exclude == []) and (.bypass_actors == [])' "$PIN_RULESET" >/dev/null
```

```bash
set -euo pipefail
assert_live_pin_guard() {
  test "$(git -C "$SWARM_ROOT" ls-remote origin "refs/tags/$PIN_TAG" | awk '{print $1}')" = "$SWARM_PARENT"
  gh api "repos/EffortlessMetrics/ripr-swarm/rulesets/${PIN_RULESET_ID}" > "$PACKET_ROOT/pin-ruleset-live.json"
  cmp "$PIN_RULESET" "$PACKET_ROOT/pin-ruleset-live.json"
  test "$(sha256sum "$PACKET_ROOT/pin-ruleset-live.json" | awk '{print $1}')" = "$(sha256sum "$PIN_RULESET" | awk '{print $1}')"
  test "$(sha256sum "$PACKET_ROOT/pin-remote.sha" | awk '{print $1}')" = "$PIN_DIGEST"
}
```

The exact ruleset receipt is a prerequisite, not a suggestion. If the named
active tag ruleset, its exact `refs/tags/ripr-release-*` pattern, or both
update/deletion rules are absent, stop before W; do not substitute an
unprotected custom ref.

```bash
set -euo pipefail
# [EXTERNAL-PUBLISHING] repo=swarm remote; publish protected candidate tag only
PIN_BEFORE="$(git -C "$SWARM_ROOT" ls-remote origin "refs/tags/$PIN_TAG" | awk '{print $1}')"
test -z "$PIN_BEFORE"
test -z "$(git -C "$SWARM_ROOT" show-ref --tags --verify "refs/tags/$PIN_TAG" 2>/dev/null || true)"
git -C "$SWARM_ROOT" tag "$PIN_TAG" "$SWARM_PARENT"
git -C "$SWARM_ROOT" push origin "refs/tags/$PIN_TAG:refs/tags/$PIN_TAG"
PIN_AFTER="$(git -C "$SWARM_ROOT" ls-remote origin "refs/tags/$PIN_TAG" | awk '{print $1}')"
test "$PIN_AFTER" = "$SWARM_PARENT"
printf '%s\n' "$PIN_AFTER" > "$PACKET_ROOT/pin-remote.sha"
PIN_DIGEST="$(sha256sum "$PACKET_ROOT/pin-remote.sha" | awk '{print $1}')"
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; assemble the one schema-1.1 manifest after W
# These are existing accepted owner packets, not files synthesized to make this step pass.
SELECTED_CLAIMS_PACKET="${SELECTED_CLAIMS_PACKET:?set controller-relative accepted #2766 packet}"
DENOMINATOR_PACKET="${DENOMINATOR_PACKET:?set controller-relative accepted #2768 packet}"
AUDIT_PACKET="${AUDIT_PACKET:?set controller-relative accepted #3807 packet}"
# #2768 supplies the reviewed schema-1.1 projection: topo-order/reverse and
# first-parent/reverse full-SHA+LF digests from SPEC-0144, not historical JSON
# hashes, plus the NUL-framed path-record count/digest/recipe over the same
# range (#4006 item 6); admission recomputes all of it live.
DENOMINATOR_RANGE_JSON="${DENOMINATOR_RANGE_JSON:?set accepted #2768 range projection}"
# References to the selected candidate's exact proof-input bytes, using SPEC-0144 Evidence fields.
PROOF_INPUTS_JSON="${PROOF_INPUTS_JSON:?set accepted proof-input reference list}"
# Supplied by the trusted controller from exact native owner decisions, not generated here.
OWNER_ACCEPTANCE_JSON="${OWNER_ACCEPTANCE_JSON:?set reviewed selection/denominator/audit acceptance envelopes}"
CONTROLLER_ROOT="${CONTROLLER_ROOT:?set absolute independent controller root}"
# The producer resolves --input against CONTROLLER_ROOT, so jq writes there.
MANIFEST_INPUT="${MANIFEST_INPUT:-live-head-selection-input.json}"
CANDIDATE_TREE="$(git -C "$SWARM_ROOT" rev-parse "$SWARM_PARENT^{tree}")"
test "$(git -C "$SWARM_ROOT" rev-parse HEAD)" = "$SWARM_PARENT"
test -z "$(git -C "$SWARM_ROOT" status --porcelain=v1 --untracked-files=all)"
# Copy readbacks into this controller's packet; paths remain relative and bytes are retained.
PIN_READBACK_PATH="${PIN_READBACK_PATH:?set controller-relative pin-remote.sha}"
PIN_RULESET_PATH="${PIN_RULESET_PATH:?set controller-relative pin-ruleset.json}"
jq --arg sha "$SWARM_PARENT" --arg tree "$CANDIDATE_TREE" --arg ref "$SWARM_REF" \
  --arg workspace_sha "$(git -C "$SWARM_ROOT" show "$SWARM_PARENT:Cargo.toml" | sha256sum | awk '{print $1}')" \
  --arg package_sha "$(git -C "$SWARM_ROOT" show "$SWARM_PARENT:crates/ripr/Cargo.toml" | sha256sum | awk '{print $1}')" \
  --arg lock_sha "$(git -C "$SWARM_ROOT" show "$SWARM_PARENT:Cargo.lock" | sha256sum | awk '{print $1}')" \
  --arg selection "$SELECTED_CLAIMS_PACKET" --arg selection_sha "$(sha256sum "$CONTROLLER_ROOT/$SELECTED_CLAIMS_PACKET" | awk '{print $1}')" \
  --arg denominator "$DENOMINATOR_PACKET" --arg denominator_sha "$(sha256sum "$CONTROLLER_ROOT/$DENOMINATOR_PACKET" | awk '{print $1}')" \
  --arg audit "$AUDIT_PACKET" --arg audit_sha "$(sha256sum "$CONTROLLER_ROOT/$AUDIT_PACKET" | awk '{print $1}')" \
  --arg readback "$PIN_READBACK_PATH" --arg readback_sha "$(sha256sum "$CONTROLLER_ROOT/$PIN_READBACK_PATH" | awk '{print $1}')" \
  --arg ruleset "$PIN_RULESET_PATH" --arg ruleset_sha "$(sha256sum "$CONTROLLER_ROOT/$PIN_RULESET_PATH" | awk '{print $1}')" \
  --slurpfile range "$DENOMINATOR_RANGE_JSON" --slurpfile proof "$PROOF_INPUTS_JSON" --slurpfile accepted "$OWNER_ACCEPTANCE_JSON" '
  .status = "pinned_exact_head" |
  .candidate = {repository: "EffortlessMetrics/ripr-swarm", sha: $sha, tree: $tree, ref: $ref,
    package: {name: "ripr", version: .release_line, workspace_manifest_sha256: $workspace_sha,
      package_manifest_sha256: $package_sha, lock_sha256: $lock_sha}} |
  .range = $range[0] |
  .prerequisites = {selected_claims: {packet: {owner_issue: 2766, path: $selection, sha256: $selection_sha}, acceptance: $accepted[0].selected_claims},
    denominator: {packet: {owner_issue: 2768, path: $denominator, sha256: $denominator_sha}, acceptance: $accepted[0].denominator},
    audit: {packet: {owner_issue: 3807, path: $audit, sha256: $audit_sha}, acceptance: $accepted[0].audit}} |
  .pin = {remote_ref_readback: {owner_issue: 1609, path: $readback, sha256: $readback_sha},
    ruleset: {owner_issue: 1609, path: $ruleset, sha256: $ruleset_sha}} |
  .qualification.proof_inputs = $proof[0] |
  .source_parent = null' "$PACKET_ROOT/live-head-selection-template.json" > "$CONTROLLER_ROOT/$MANIFEST_INPUT"
# jq only assembles proposed inputs. The existing Rust DTO validates all typed
# fields and retained owner/pin bytes, then emits both projections. OUT must be
# a new directory below CONTROLLER_ROOT; it is not a standalone controller root.
MANIFEST_OUTPUT="${MANIFEST_OUTPUT:?set new controller-relative proposal directory}"
cargo xtask release-manifest-proposal --controller-root "$CONTROLLER_ROOT" \
  --version 0.11.0 --input "$MANIFEST_INPUT" --out "$MANIFEST_OUTPUT"
# Keep the original controller root for consumers: evidence paths are unchanged.
PACKET_MANIFEST="$CONTROLLER_ROOT/$MANIFEST_OUTPUT/manifest.json"
# STOP: retain this proposed output for #1609 acceptance. Hashing it is not approval.
# Only after independent #1609 review, the trusted release operator supplies the accepted digest.
ACCEPTED_MANIFEST_SHA256="${ACCEPTED_MANIFEST_SHA256:?obtain from the reviewed #1609 handoff, not this producer}"
test "$(sha256sum "$PACKET_MANIFEST" | awk '{print $1}')" = "$ACCEPTED_MANIFEST_SHA256"
```

At every later receipt, read `refs/tags/$PIN_TAG` again and require the
recorded SHA and the exact ruleset receipt to remain unchanged. The protected
candidate tag, not the local verifier ref, is the W membership pin.

The manifest records the accepted #2768 boundary, separately named
all-reachable and first-parent counts/digests, record-set identity, the #2768
ledger's raw NUL-framed changed-path count/digest/recipe over the same range
(#4006 item 6), exact
candidate tree/ref/package, accepted owner packets and claims/non-claims.
It does not predict SOURCE_PARENT or the later source-specific merge base.
The schema-1.1 range uses the shared source-promotion recipe in SPEC-0144,
with the #2768 boundary rather than the later source-specific merge base.
Admission recomputes both Git ranges/counts/digests live and compares the
path-record identity typed against the retained #2768 packet bytes for the
same base and cut; a substituted digest,
a copied count, an LF-joined digest, a foreign recipe or a packet that drops
the inventory refuses the manifest;
record-set adjudication remains with #2768. The retained input budget is 64 proof references, 16 MiB
per file and 64 MiB aggregate. Supported origin strings are the exact three
spellings checked above. Files are observed unlocked snapshots, not atomic
or authenticated custody. Repin only for a release-invalidating exact-candidate
semantic/policy failure or source-preflight survivor failure. Main movement
alone never repins. Changed inputs supersede the packet and require a new one.

## 3. Exact swarm qualification and source preflight

Qualification is a detached worktree at `SWARM_PARENT`; a hosted result must
report that exact SHA as `headSha`.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=swarm root; create disposable exact-head worktree
QUAL_ROOT="${SWARM_ROOT}-release-${VERSION}-qual"
git -C "$SWARM_ROOT" worktree add --detach "$QUAL_ROOT" "$SWARM_PARENT"
test "$(git -C "$QUAL_ROOT" rev-parse HEAD)" = "$SWARM_PARENT"
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=swarm qualification worktree; checks write local reports
assert_live_pin_guard
git -C "$QUAL_ROOT" status --short --branch
git -C "$QUAL_ROOT" rev-parse HEAD
(cd "$QUAL_ROOT" && cargo xtask check-pr)
(cd "$QUAL_ROOT" && cargo xtask check-generated-clean)
(cd "$QUAL_ROOT" && cargo xtask check-doc-index)
# These are preparation checks only. Do not manufacture an acceptance receipt
# from an arbitrary successful hosted run or its head SHA.
# #2769 must independently review the complete exact-candidate bundle first.
SELECTION_DECISION="${SELECTION_DECISION:?native #1609 acceptance-comment URL}"
QUALIFICATION_DECISION="${QUALIFICATION_DECISION:?native #2769 complete-bundle acceptance-comment URL}"
QUALIFICATION_BUNDLE="${QUALIFICATION_BUNDLE:?controller-relative accepted complete qualification bundle}"
```

A missing, timed-out, or differently headed hosted result is unavailable, not
a pass. A generic successful CI run is never the #2769 acceptance record.
The native decision and bundle field contract is in
[SOURCE_PROMOTION_PREFLIGHT.md](SOURCE_PROMOTION_PREFLIGHT.md#native-selection-and-complete-qualification-admission). After #2769 accepts the complete exact-candidate bundle, public-return admission
(`ripr#1771` under `ripr#1627`/`#1889`/`#1762`) rereads and dispositions the
source queue, confirms public `P` has not moved and is an ancestor of
`SWARM_PARENT`, and binds `SOURCE_PARENT`:

```bash
set -euo pipefail
# [READ-ONLY] repo=source; performed only after complete swarm qualification
SOURCE_PARENT="$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)"
test "$(git -C "$SOURCE_ROOT" rev-parse HEAD)" = "$SOURCE_PARENT"
test -z "$(git -C "$SOURCE_ROOT" status --porcelain=v1 --untracked-files=all)"
# [LOCAL-MUTATING] repo=packet; distinct later source transaction identity
printf '%s\n' "$SOURCE_PARENT" > "$PACKET_ROOT/SOURCE_PARENT"
MERGE_BASE="$(git -C "$SWARM_ROOT" merge-base "$SOURCE_PARENT" "$SWARM_PARENT")"
```

Source movement after this hold requires a new source generation. It does not
retroactively mutate the swarm manifest. Run the existing exact-pair preflight with exact SHA declarations; its
contract is [`SOURCE_PROMOTION_PREFLIGHT.md`](SOURCE_PROMOTION_PREFLIGHT.md)
and [`RIPR-SPEC-0148`](specs/RIPR-SPEC-0148-source-promotion-preflight.md).
The resolution reviewer produces `JOIN_TREE` as a full tree SHA in the
resolution manifest; the automatic `preview_tree` is never substituted.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=swarm operator checkout; preflight writes local receipts, no J
JOIN_TREE="${JOIN_TREE:?set to the separately reviewed full resolved-tree SHA from the resolution manifest}"
assert_live_pin_guard
(cd "$SWARM_ROOT" && cargo xtask source-promotion preflight \
  --source-parent "$SOURCE_PARENT" --swarm-parent "$SWARM_PARENT" \
  --swarm-ref "$SWARM_REF" --source-repo "$SOURCE_ROOT" --swarm-repo "$SWARM_ROOT" \
  --source-main "$SOURCE_PARENT" --swarm-main "$SWARM_PARENT" \
  --version "$VERSION" --resolved-tree "$JOIN_TREE" \
  --controller-root "$CONTROLLER_ROOT" --candidate-manifest "$MANIFEST_OUTPUT/manifest.json" \
  --selection-decision "$SELECTION_DECISION" \
  --qualification-bundle "$QUALIFICATION_BUNDLE" \
  --qualification-decision "$QUALIFICATION_DECISION" \
  --out "$PACKET_ROOT/source-promotion")
PREFLIGHT_JSON="$PACKET_ROOT/source-promotion/source-promotion-preflight.json"
test "$(jq -r '.dry_merge.reviewed_resolved_tree // empty' "$PREFLIGHT_JSON")" = "$JOIN_TREE"
```

The v2 receipt cannot be supplied to a v1-only source verifier. Public-return
admission remains held until
[ripr#1627](https://github.com/EffortlessMetrics/ripr/issues/1627) /
[ripr#1889](https://github.com/EffortlessMetrics/ripr/pull/1889) and
[ripr#1771](https://github.com/EffortlessMetrics/ripr/issues/1771) deliver
trusted v2 qualification admission and guarded merge-composition verification;
no v1 fallback waives it.

Review every conflict path, survivor, swarm exclusion, authority candidate,
and separately reviewed `JOIN_TREE`; record a resolution manifest. A clean
textual merge is not semantic approval. Parent, ref, identity, ancestry,
digest, conflict, or tree changes invalidate the receipt.

## 4. Guarded public return (`ripr#1771` → `ripr#1772`/`#1773`)

Do **not** construct or merge `J` with raw `git merge`, a direct
`promote/${VERSION}-swarm` branch push, or a manual `gh pr merge` command.
Under `#2379`, `ripr#1627`/`#1889`, and `ripr#1771`–`#1773`, raw `git merge`
construction is rejected, and a manual merge PR creates an unintended
double-merge instead of the guarded return composition.

For `0.11.0`, the source repository's trusted/guarded constructor and merge
route owns this step:

1. **`ripr#1771` (qualification admission):** verifies the `#2769` qualification
   receipt against the exact pinned `SWARM_PARENT` and confirms public
   `SOURCE_PARENT` (`P`) has not moved and is an ancestor of `SWARM_PARENT`.
2. **`ripr#1772` (guarded construction):** builds and publishes the reviewed
   return candidate via the source repository's guarded constructor
   (`source-promotion admit-resolved-tree`, `construct-exact-join`,
   `publish-candidate-ref`; see `EffortlessMetrics/ripr`
   `docs/SOURCE_PROMOTION.md` and
   [`swarm-development.md`](swarm-development.md#resolving-the-join)).
3. **`ripr#1773` (guarded merge and proof):** merges the exact reviewed return
   under expected-head and expected-base guards, preserving both histories and
   the reviewed tree `JOIN_TREE` (`T`).

Required graph invariants for the guarded return (replacing the historical
direct-`J` shell blocks):

```text
J.parent[0] = SOURCE_PARENT
J.parent[1] = SWARM_PARENT
J^{tree}    = JOIN_TREE
```

The exact-J verifier contract originates from
[#1493](https://github.com/EffortlessMetrics/ripr/issues/1493) (merged at
[`9c53b12b`](https://github.com/EffortlessMetrics/ripr/commit/9c53b12b73aa5a11c198f8c5265902127a6d7dff))
and is bound to trusted public-return admission under
[ripr#1627](https://github.com/EffortlessMetrics/ripr/issues/1627) /
[ripr#1889](https://github.com/EffortlessMetrics/ripr/pull/1889). Never append
a repair commit; squash, rebase, cherry-pick, and tree-equivalent
reconstruction fail the contract. Rebuild through `ripr#1772` from the held
exact pair after any changed resolution or receipt.

After `ripr#1773` completes and lands on `EffortlessMetrics/ripr` `main`,
bind and verify the resulting `SOURCE_JOIN_HEAD` (read-only):

```bash
set -euo pipefail
# [READ-ONLY] repo=source remote; verify guarded public-return result after ripr#1773
assert_live_pin_guard
git -C "$SOURCE_ROOT" fetch origin main
SOURCE_JOIN_HEAD="$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)"
git -C "$SOURCE_ROOT" merge-base --is-ancestor "$SOURCE_PARENT" "$SOURCE_JOIN_HEAD"
git -C "$SOURCE_ROOT" merge-base --is-ancestor "$SWARM_PARENT" "$SOURCE_JOIN_HEAD"
test "$(git -C "$SOURCE_ROOT" rev-parse "$SOURCE_JOIN_HEAD^{tree}")" = "$JOIN_TREE"
```

## 5. Metadata and source artifact qualification

J carries the promoted graph. Version/changelog metadata is a separate source
release-preparation change after J reaches source main; never bump J.

> **0.11.0:** the version write already landed in ripr#1708, and
> `bump-version` rejects a no-op bump, so skip the `bump-version` line below.
> Per ripr#1466, do not open a replacement metadata PR. The release-copy PR
> that ripr#1466 owns (folding `Unreleased` into the staged `0.11.0` section)
> takes its place, and `SOURCE_RELEASE_HEAD` is source `main` after it merges.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=source release-prep checkout at source join result; metadata only
git -C "$SOURCE_ROOT" switch -c "release/${VERSION}" "$SOURCE_JOIN_HEAD"
(cd "$SOURCE_ROOT" && cargo xtask bump-version "$VERSION")
git -C "$SOURCE_ROOT" add Cargo.toml Cargo.lock crates docs
git -C "$SOURCE_ROOT" commit -m "chore(release): bump ${VERSION} metadata"
git -C "$SOURCE_ROOT" push origin "release/${VERSION}"
SOURCE_METADATA_PR="$(gh pr create --repo EffortlessMetrics/ripr --head "release/${VERSION}" --base main --title "chore(release): bump ${VERSION} metadata" --body "Release metadata only; expected source join ${SOURCE_JOIN_HEAD}")"
SOURCE_METADATA_PR_NUMBER="${SOURCE_METADATA_PR##*/}"
gh pr view "$SOURCE_METADATA_PR_NUMBER" --repo EffortlessMetrics/ripr --json headRefOid --jq .headRefOid | tee "$PACKET_ROOT/source-metadata-pr-head.sha"
test "$(cat "$PACKET_ROOT/source-metadata-pr-head.sha")" = "$(git -C "$SOURCE_ROOT" rev-parse HEAD)"
gh pr merge "$SOURCE_METADATA_PR_NUMBER" --repo EffortlessMetrics/ripr --squash --match-head-commit "$(cat "$PACKET_ROOT/source-metadata-pr-head.sha")"
```

Set `SOURCE_RELEASE_HEAD` to the exact source-main SHA after that PR. The ship
packet needs fresh evidence for all of these, bound to that SHA:

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=source+packet; produce the release-head binding after metadata merge
git -C "$SOURCE_ROOT" fetch origin main
SOURCE_RELEASE_HEAD="$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)"
test "$(git -C "$SOURCE_ROOT" rev-parse "$SOURCE_RELEASE_HEAD^{commit}")" = "$SOURCE_RELEASE_HEAD"
git -C "$SOURCE_ROOT" switch --detach "$SOURCE_RELEASE_HEAD"
test -z "$(git -C "$SOURCE_ROOT" status --short)"
(cd "$SOURCE_ROOT" && cargo xtask release-readiness --version "$VERSION")
(cd "$SOURCE_ROOT" && cargo xtask release-negative-corpus --version "$VERSION")
# zero-case guard: a green exit with no executed or with failed cases is not proof
jq -e '.summary.total_cases > 0 and .summary.failed == 0' \
  "$SOURCE_ROOT/target/ripr/reports/release-negative-corpus.json"
```

The version-only `release-negative-corpus` run is legacy smoke
(`qualification_mode: legacy_smoke_unqualified`). For the #1609-qualified path
add the complete `--controller-root`, `--candidate-source-root`,
`--candidate-artifact` group described in [`RELEASE.md`](RELEASE.md) and record
`qualification_mode` in the packet; the qualified form writes the report under `<controller-root>/target/ripr/reports/`, so point the `jq` guard there. Do not claim controlled negative injections
for the candidate from the unqualified form (#2792).

| Surface | Repository/worktree and mode | Proof | Boundary |
| Crate | source exact release checkout, **[LOCAL-MUTATING]** for package output; **[READ-ONLY]** dry-run query | `(cd "$SOURCE_ROOT" && cargo package -p ripr --list)`; `(cd "$SOURCE_ROOT" && cargo xtask publish-dry-run)`, which packages both crates from the publish stage (inline `#[cfg(test)]` modules stripped, [`RELEASE.md`](RELEASE.md#crate-tarball-contents)), fails above the crates.io upload limit, and keeps the verified stage at `target/publish-stage` | Package/publishability from the stage that is published, not publication. A bare `cargo publish -p ripr --dry-run` from the checkout packages the unstripped crate, which is over the limit |
| Installed CLI | source exact release checkout, **[LOCAL-MUTATING]** | `(cd "$SOURCE_ROOT" && cargo install --path crates/ripr --locked --force --root target/ripr/install-smoke)` plus version/doctor/fixture smoke | Local installed binary |
| Readiness/consumer | source exact release checkout, **[LOCAL-MUTATING]** | `(cd "$SOURCE_ROOT" && cargo xtask release-readiness --version <VERSION>)` | Version alignment and journey receipt |
| Negative corpus | source exact release checkout, **[LOCAL-MUTATING]** | `(cd "$SOURCE_ROOT" && cargo xtask release-negative-corpus --version <VERSION>)` plus the `summary` guard above | Controlled negative injections fail for intended reasons and restoration passes (#2792); smoke unless the qualified group is passed |
| Linux/Windows | source release workflow artifacts, **[READ-ONLY]** when inspecting exact `SOURCE_RELEASE_HEAD` outputs | Workflow artifacts bound to `SOURCE_RELEASE_HEAD` | Platform proof; one OS is not the other |
| Server | source exact release checkout, **[LOCAL-MUTATING]** | `(cd "$SOURCE_ROOT" && cargo xtask release-server-archive ...)`; `(cd "$SOURCE_ROOT" && cargo xtask release-server-manifest ...)` | Archive, manifest, checksum shape |
| VSIX | source exact release checkout, **[LOCAL-MUTATING]** | `npm --prefix editors/vscode ci`, compile, package | Local package/version proof |
| Badge | source exact release checkout, **[LOCAL-MUTATING]** | `(cd "$SOURCE_ROOT" && cargo xtask repo-badge-artifacts)` plus generated-clean/badge policy checks | Generated endpoint/freshness proof |

Use [`RELEASE_BINARIES.md`](RELEASE_BINARIES.md),
[`RELEASE_MARKETPLACE.md`](RELEASE_MARKETPLACE.md),
[`INSTALLATION_VERIFICATION.md`](INSTALLATION_VERIFICATION.md), and
[`BADGE_POLICY.md`](BADGE_POLICY.md) for surface semantics. Record exact SHA,
command, status, artifact path, and limitation; never copy an old blanket
no-execution sentence.

## 6. Ship packet, public RC, #1470 authorization, channels, and verification

The packet is evidence, not permission. Two source-owned gates stand between
the ship packet and any stable channel:

```text
ripr#1469 ship
→ ripr#1634 exact authorized GitHub-only RC
→ ripr#1635 independent public-byte/install verification
→ ripr#1637 stable decision = stable_ready_same_candidate
→ ripr#1470 exact per-channel authorization record
→ stable channels, one at a time
```

### Public RC leg (`ripr#1632`)

[ripr#1632](https://github.com/EffortlessMetrics/ripr/issues/1632) requires
one exact public RC, published and independently verified, before #1470 may
authorize crates.io, the stable tag, either marketplace, or any other stable
channel. This runbook carries no RC publishing template. The RC is published
only by the source-owned transaction under `ripr#1634`, on the exact ref and
owner ruling reconciled by `ripr#1847`/`#1632`. That ruling is
GitHub-prerelease-only: it reaches no stable channel, no registry, no
marketplace, and no other RC ordinal. Do not push an RC tag by hand, run a
stable template below against an RC ref, or start K from an RC.

The governed repair cohort (`ripr#1633`/`#1636`) is optional evidence under
#1632's 2026-11-11 programme comment. A severe defect it observes still bears
on `ripr#1637`; a missing cohort does not hold the RC or stable.

Stop before the #1470 step unless all of these hold for the same
`SOURCE_RELEASE_HEAD`:

- the RC tag is annotated, peels to `SOURCE_RELEASE_HEAD`, and its GitHub
  Release is a published prerelease;
- `ripr#1635` is terminal on the public bytes, not on a workspace build or a
  private qualification artifact;
- the terminal `ripr#1637` receipt records exactly
  `stable_ready_same_candidate`. `repair_and_publish_next_rc`,
  `remain_prerelease`, or `do_not_ship_stable` keeps every stable channel
  `not_authorized`, and a green ship packet does not override it.

The RC tag is immutable once it exists. A candidate, workflow, or asset defect
goes through a new candidate, a new ship packet, and renewed authority for the
next exact RC; never move, delete, or recreate the tag or replace its assets.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; fail closed unless the exact public RC and the #1637 decision exist
RC_REF="${RC_REF:?set to the exact RC tag reconciled by ripr#1847/#1632}"
STABLE_DECISION_RECEIPT="${STABLE_DECISION_RECEIPT:?set to the retained terminal ripr#1637 decision receipt}"
case "$RC_REF" in "v${VERSION}-rc."[0-9]*) ;; *) echo "$RC_REF is not an RC ref for $VERSION" >&2; exit 1 ;; esac
# `^{}` resolves only for an annotated tag; a lightweight RC tag fails here by design.
test "$(git -C "$SOURCE_ROOT" ls-remote origin "refs/tags/${RC_REF}^{}" | awk '{print $1}')" = "$SOURCE_RELEASE_HEAD"
gh release view "$RC_REF" --repo EffortlessMetrics/ripr --json tagName,isDraft,isPrerelease,assets,url > "$PACKET_ROOT/rc-release-receipt.json"
jq -e --arg tag "$RC_REF" '.tagName == $tag and .isDraft == false and .isPrerelease == true' "$PACKET_ROOT/rc-release-receipt.json" >/dev/null
# Coarse token guard on the decision; the receipt's own verifier stays with ripr#1637.
grep -Fq stable_ready_same_candidate "$STABLE_DECISION_RECEIPT"
test "$(grep -Ec 'repair_and_publish_next_rc|remain_prerelease|do_not_ship_stable' "$STABLE_DECISION_RECEIPT" || true)" = 0
STABLE_DECISION_RECEIPT_SHA256="$(sha256sum "$STABLE_DECISION_RECEIPT" | cut -d' ' -f1)"
printf '%s\n' "$STABLE_DECISION_RECEIPT_SHA256" > "$PACKET_ROOT/stable-decision-receipt.sha256"
```

### Stable authorization record (`ripr#1470`)

The #1470 record is a maintainer comment on the issue, not the issue body. It
names the exact candidate and ship packet and states `authorized` or
`not_authorized` independently for every item. Authorization for one item does
not imply another. A missing, ambiguous, duplicated, or edited line is
`not_authorized`, and so is a record the issuer has followed with a later
comment on #1470. The check below reads these lines, with `=` or `:` as the
separator, as plain lines, list items, or table rows:

```text
version = <VERSION>
SOURCE_RELEASE_HEAD = <sha>
SOURCE_RELEASE_TREE = <sha>
SHIP_PACKET_PR = <number>
SHIP_PACKET_SHA256 = <sha256>
STABLE_DECISION_RECEIPT_SHA256 = <sha256>
crates.io publication = authorized | not_authorized
v<VERSION> tag creation/push = authorized | not_authorized
GitHub Release creation/update = authorized | not_authorized
server archive/manifest/SHA256SUMS upload = authorized | not_authorized
VSIX GitHub asset upload when applicable = authorized | not_authorized
VS Code Marketplace publication = authorized | not_authorized
Open VSX publication = authorized | not_authorized
```

This line check is an operator pre-flight on the retained record. It is not
the authority path. #1470's amendments place authority in canonical packets
(`ripr#1651`), bound to an exact tag or continuation event (`ripr#1652`) and
consumed by one publication owner (`ripr#1646`, with `ripr#1644`/`#1649`), and
say the record cannot authorize `cargo publish`, `gh release`, or another
workflow generically. A template below runs only when the record names that
exact command or workflow as the channel's path. Otherwise stop: the channel
belongs to the source transaction the record names. `ripr#1637`'s 2026-08-30
amendment also makes that bridge a precondition of
`stable_ready_same_candidate`; its 2026-11-11 floor does not restate it, and
this runbook does not settle which holds.

The check also does not read the remaining RC evidence digests,
artifact-attestation permissions, or each release secret/environment. Read
those rows in the retained record before each leaf.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; capture the exact #1470 authorization comment and public state
assert_live_pin_guard
AUTHORIZATION_COMMENT_ID="${AUTHORIZATION_COMMENT_ID:?set to the numeric id of the ripr#1470 authorization comment}"
# #1470 reserves the record to the release owner; organization membership is not enough.
AUTHORIZATION_ISSUER=EffortlessSteven
SHIP_PACKET_PR="${SHIP_PACKET_PR:?set to the ripr#1469 ship-packet PR number}"
SHIP_PACKET="${SHIP_PACKET:?set to the retained immutable ripr#1469 ship packet}"
SHIP_PACKET_SHA256="$(sha256sum "$SHIP_PACKET" | cut -d' ' -f1)"
SOURCE_RELEASE_TREE="$(git -C "$SOURCE_ROOT" rev-parse "${SOURCE_RELEASE_HEAD}^{tree}")"
AUTHORIZATION_ISSUE="$PACKET_ROOT/publication-authorization-issue.json"
AUTHORIZATION_RECEIPT="$PACKET_ROOT/publication-authorization.json"
AUTHORIZATION_BODY="$PACKET_ROOT/publication-authorization.md"
gh api repos/EffortlessMetrics/ripr/issues/1470 > "$AUTHORIZATION_ISSUE"
gh api "repos/EffortlessMetrics/ripr/issues/comments/${AUTHORIZATION_COMMENT_ID}" > "$AUTHORIZATION_RECEIPT"
jq -r .body "$AUTHORIZATION_RECEIPT" > "$AUTHORIZATION_BODY"
sha256sum "$AUTHORIZATION_BODY" | cut -d' ' -f1 > "$PACKET_ROOT/publication-authorization-body.sha256"
gh release list --repo EffortlessMetrics/ripr --limit 5 > "$PACKET_ROOT/releases-before.txt"
```

```bash
set -euo pipefail
# [READ-ONLY] repo=source/public services; fail closed on the #1470 record
authorization_value() {
  # Prints the value of the one line that starts with this exact label.
  # No such line, more than one, or one whose value is not a single token fails.
  tr -d '\r' < "$AUTHORIZATION_BODY" | sed -e 's/[|*`]/ /g' -e 's/^[[:space:]]*-[[:space:]]//' |
    awk -v label="$1" '
      { sub(/^[ \t]+/, ""); sub(/[ \t]+$/, "") }
      index($0, label) == 1 {
        rest = substr($0, length(label) + 1)
        if (rest !~ /^[ \t:=]/) next
        sub(/^[ \t]*[:=]?[ \t]*/, "", rest)
        count += 1
        if (rest == "" || rest ~ /[ \t]/) { bad = 1; next }
        value = rest
      }
      END { if (count != 1 || bad) exit 1; print value }'
}
assert_authorization_current() {
  # #1470: re-read the record before every irreversible action; an edit invalidates it.
  # Any later issuer comment on #1470 may supersede or withdraw the record: stop and rebind.
  gh api "repos/EffortlessMetrics/ripr/issues/comments/${AUTHORIZATION_COMMENT_ID}" --jq .body > "$PACKET_ROOT/publication-authorization-live.md" &&
    cmp "$AUTHORIZATION_BODY" "$PACKET_ROOT/publication-authorization-live.md" &&
    gh api --paginate --slurp "repos/EffortlessMetrics/ripr/issues/1470/comments?per_page=100" > "$PACKET_ROOT/publication-authorization-thread.json" &&
    jq -e --arg issuer "$AUTHORIZATION_ISSUER" --argjson id "$AUTHORIZATION_COMMENT_ID" '
      [.[][]] | any(.id == $id) and ([.[] | select(.user.login == $issuer and .id > $id)] | length == 0)
    ' "$PACKET_ROOT/publication-authorization-thread.json" >/dev/null
}
require_authorized() {
  assert_authorization_current && test "$(authorization_value "$1")" = authorized
}
test "$(jq -r .state "$AUTHORIZATION_ISSUE")" = open
jq -e --arg issuer "$AUTHORIZATION_ISSUER" '(.issue_url | endswith("/repos/EffortlessMetrics/ripr/issues/1470")) and
  .user.login == $issuer and .created_at == .updated_at' "$AUTHORIZATION_RECEIPT" >/dev/null
assert_authorization_current
test "$(authorization_value version)" = "$VERSION"
test "$(authorization_value SOURCE_RELEASE_HEAD)" = "$SOURCE_RELEASE_HEAD"
test "$(authorization_value SOURCE_RELEASE_TREE)" = "$SOURCE_RELEASE_TREE"
RECORDED_SHIP_PACKET_PR="$(authorization_value SHIP_PACKET_PR)"
test "${RECORDED_SHIP_PACKET_PR#\#}" = "${SHIP_PACKET_PR#\#}"
test "$(authorization_value SHIP_PACKET_SHA256)" = "$SHIP_PACKET_SHA256"
test "$(authorization_value STABLE_DECISION_RECEIPT_SHA256)" = "$STABLE_DECISION_RECEIPT_SHA256"
git -C "$SOURCE_ROOT" rev-parse "$SOURCE_RELEASE_HEAD"
```

Publish one channel at a time, in #1470's order, and receipt it before the
next: crates.io publish and verify, then the `v<VERSION>` tag, then the GitHub
Release and server assets, then the VSIX and each marketplace. Each template
calls `require_authorized` for its own item and stops when that item is not
exactly `authorized`. These templates require #1470; they are not permission
to publish from this documentation PR.

crates.io and the tag are immutable boundaries. Before either exists, a
mismatch stops the transaction. After either exists, never republish, move, or
recreate it; fix forward on source `main` under renewed exact authority.

The crate is published from the publish stage, not from `SOURCE_ROOT`: the
checkout's `ripr` package is over the crates.io upload limit (#14198). Rehearse
on the release head immediately before publishing and bind the kept stage to
it; `ripr-contract` publishes first because `ripr` requires it exactly
(ADR 0027).

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=source; rehearse and bind the publish stage to the release head
assert_live_pin_guard
test "$(git -C "$SOURCE_ROOT" rev-parse HEAD)" = "$SOURCE_RELEASE_HEAD"
(cd "$SOURCE_ROOT" && cargo xtask publish-dry-run)
PUBLISH_STAGE="$SOURCE_ROOT/target/publish-stage"
test "$(git -C "$PUBLISH_STAGE" rev-parse HEAD)" = "$SOURCE_RELEASE_HEAD"
test "$(tr -d '\r\n' < "$PUBLISH_STAGE/crates/ripr/.ripr-publish-stage")" = "$SOURCE_RELEASE_HEAD"
PUBLISH_RECEIPT="$PACKET_ROOT/publish-receipt.json"
cp "$SOURCE_ROOT/target/package/publish-receipt.json" "$PUBLISH_RECEIPT"
verify_publish_receipt() {
  cmp "$PUBLISH_RECEIPT" "$SOURCE_ROOT/target/package/publish-receipt.json" &&
    jq -es --arg v "$VERSION" --arg h "$SOURCE_RELEASE_HEAD" '
      length == 1 and (.[0] | .version == $v and .source_commit == $h and .stage_commit == $h and
        .source_dirty == false and .verification == "packaged-check-passed" and
        (.contract_sha256 | test("^[0-9a-f]{64}$")) and (.ripr_sha256 | test("^[0-9a-f]{64}$")))
    ' "$PUBLISH_RECEIPT" >/dev/null
}
verify_publish_receipt
```

Use the following steps for a fresh publication. If either version already exists,
or a previous upload has an uncertain outcome, start with
[partial-publication recovery](RELEASE.md#partial-publication-recovery). Retain
each registry response in the packet. Poll a missing version for up to six
attempts, then stop. A returned version with different metadata stops immediately.
When resuming in a fresh shell, restore the pinned paths and helper functions,
set `PUBLISH_RECEIPT="$PACKET_ROOT/publish-receipt.json"`, and use the retained
receipt and stage. Re-running the rehearsal would replace the evidence for an
upload that already happened.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; record a read-only crates.io version query
verify_registry_response() {
  jq -es --arg crate "$1" --arg v "$VERSION" --arg sha "$2" '
    length == 1 and (.[0].version |
      .crate == $crate and .num == $v and .checksum == $sha and .yanked == false)
  ' "$3" >/dev/null
}
record_crate_version() {
  local crate="$1" expected_sha="$2" response status attempt
  response="$(mktemp "$PACKET_ROOT/registry-${crate}-${VERSION}.XXXXXX")" || return 1
  for attempt in 1 2 3 4 5 6; do
    status="$(curl -sS --max-time 10 -o "$response" -w '%{http_code}' \
      -A "ripr-release-transaction/${VERSION} (+https://github.com/EffortlessMetrics/ripr)" \
      "https://crates.io/api/v1/crates/${crate}/${VERSION}")" || return 1
    case "$status" in
      200)
        if ! verify_registry_response "$crate" "$expected_sha" "$response"; then
          echo "Registry identity mismatch for $crate $VERSION. Expected SHA-256 $expected_sha. Inspect $response." >&2
          return 1
        fi
        mv "$response" "$PACKET_ROOT/registry-${crate}-${VERSION}.json" || return 1
        return 0 ;;
      404) if [ "$attempt" -lt 6 ]; then sleep 5; fi ;;
      *) echo "crates.io returned HTTP $status for $crate $VERSION" >&2; return 1 ;;
    esac
  done
  echo "crates.io has not reported $crate $VERSION after six attempts" >&2
  return 1
}
```

```bash
set -euo pipefail
# [EXTERNAL-PUBLISHING] repo=source publish stage; explicit #1470 crate authorization only
assert_live_pin_guard
require_authorized "crates.io publication"
verify_publish_receipt
test "$(tr -d '\r\n' < "$PUBLISH_STAGE/crates/ripr/.ripr-publish-stage")" = "$SOURCE_RELEASE_HEAD"
(cd "$PUBLISH_STAGE" && cargo publish --allow-dirty -p ripr-contract)
record_crate_version ripr-contract "$(jq -er .contract_sha256 "$PUBLISH_RECEIPT")"

assert_live_pin_guard
require_authorized "crates.io publication"
verify_publish_receipt
(cd "$SOURCE_ROOT" && cargo xtask publish-precheck)
(cd "$PUBLISH_STAGE" && cargo publish --allow-dirty -p ripr)
record_crate_version ripr "$(jq -er .ripr_sha256 "$PUBLISH_RECEIPT")"
```

#1470's first step is publish *and verify*: a clean locked install from the
registry comes before the tag, because the registry API can list both versions
while a fresh Cargo home still cannot resolve them. This is a version smoke
only; `ripr#1481` owns the full public command-surface proof.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; clean crates.io install in a fresh Cargo home and prefix, before the tag
CRATE_INSTALL_ROOT="$(mktemp -d "$PACKET_ROOT/crate-install.XXXXXX")"
CARGO_HOME="$CRATE_INSTALL_ROOT/cargo-home" cargo install ripr --version "=${VERSION}" --locked --root "$CRATE_INSTALL_ROOT/prefix"
INSTALLED_VERSION="$("$CRATE_INSTALL_ROOT/prefix/bin/ripr" --version)"
case "$INSTALLED_VERSION" in "ripr ${VERSION}" | "ripr ${VERSION} "*) ;; *) echo "installed binary reports: $INSTALLED_VERSION" >&2; exit 1 ;; esac
jq -n --arg version "$VERSION" --arg reported "$INSTALLED_VERSION" \
  --arg sha "$(sha256sum "$CRATE_INSTALL_ROOT/prefix/bin/ripr" | cut -d' ' -f1)" \
  '{version: $version, source: "crates.io", reported: $reported, executable_sha256: $sha}' > "$PACKET_ROOT/crate-install-receipt.json"
```

```bash
set -euo pipefail
# [EXTERNAL-PUBLISHING] repo=source; explicit #1470 tag authorization only, after crates.io is verified
assert_live_pin_guard
require_authorized "v${VERSION} tag creation/push"
jq -e --arg v "$VERSION" '.version == $v and .source == "crates.io" and (.executable_sha256 | test("^[0-9a-f]{64}$"))' "$PACKET_ROOT/crate-install-receipt.json" >/dev/null
verify_publish_receipt
verify_registry_response ripr-contract "$(jq -er .contract_sha256 "$PUBLISH_RECEIPT")" "$PACKET_ROOT/registry-ripr-contract-${VERSION}.json"
verify_registry_response ripr "$(jq -er .ripr_sha256 "$PUBLISH_RECEIPT")" "$PACKET_ROOT/registry-ripr-${VERSION}.json"
test -z "$(git -C "$SOURCE_ROOT" ls-remote origin "refs/tags/v${VERSION}" | awk '{print $1}')"
git -C "$SOURCE_ROOT" push origin "$SOURCE_RELEASE_HEAD:refs/tags/v${VERSION}"
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; capture the exact published tag target
gh api "repos/EffortlessMetrics/ripr/git/ref/tags/v${VERSION}" --jq .object.sha > "$PACKET_ROOT/tag-object.sha"
test "$(tr -d '\r\n' < "$PACKET_ROOT/tag-object.sha")" = "$SOURCE_RELEASE_HEAD"
```

```bash
set -euo pipefail
# [EXTERNAL-PUBLISHING] repo=source; explicit #1470 GitHub-release authorization only
assert_live_pin_guard
require_authorized "GitHub Release creation/update"
test "$(tr -d '\r\n' < "$PACKET_ROOT/tag-object.sha")" = "$SOURCE_RELEASE_HEAD"
verify_publish_receipt
verify_registry_response ripr-contract "$(jq -er .contract_sha256 "$PUBLISH_RECEIPT")" "$PACKET_ROOT/registry-ripr-contract-${VERSION}.json"
verify_registry_response ripr "$(jq -er .ripr_sha256 "$PUBLISH_RECEIPT")" "$PACKET_ROOT/registry-ripr-${VERSION}.json"
gh release create "v${VERSION}" --repo EffortlessMetrics/ripr --target "$SOURCE_RELEASE_HEAD" --title "ripr ${VERSION}" --notes-file "$RELEASE_NOTES"
```

```bash
set -euo pipefail
# [READ-ONLY] repo=source/public; receipt GitHub release before next channel
gh release view "v${VERSION}" --repo EffortlessMetrics/ripr --json tagName,targetCommitish,isDraft,isPrerelease,assets,url
gh release view "v${VERSION}" --repo EffortlessMetrics/ripr --json tagName,targetCommitish,isDraft,isPrerelease > "$PACKET_ROOT/github-release-receipt.json"
jq -e --arg tag "v${VERSION}" --arg head "$SOURCE_RELEASE_HEAD" '.tagName == $tag and .targetCommitish == $head and .isDraft == false and .isPrerelease == false' "$PACKET_ROOT/github-release-receipt.json" >/dev/null
```

```bash
set -euo pipefail
# [EXTERNAL-PUBLISHING] repo=source; explicit #1470 server-artifact authorization only
require_authorized "server archive/manifest/SHA256SUMS upload"
DISPATCHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
RELEASE_REF="v${VERSION}"
test "$(gh api "repos/EffortlessMetrics/ripr/git/ref/tags/${RELEASE_REF}" --jq .object.sha)" = "$SOURCE_RELEASE_HEAD"
gh api --paginate --slurp "repos/EffortlessMetrics/ripr/actions/workflows/release-server-binaries.yml/runs?per_page=100&event=workflow_dispatch" | jq '[.[] | .workflow_runs[]]' > "$PACKET_ROOT/server-runs-before.json"
gh workflow run release-server-binaries.yml --repo EffortlessMetrics/ripr --ref "$RELEASE_REF" -f version="$VERSION"
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; paginate and bind the dispatched server run
gh api --paginate --slurp "repos/EffortlessMetrics/ripr/actions/workflows/release-server-binaries.yml/runs?per_page=100&event=workflow_dispatch" | jq '[.[] | .workflow_runs[]]' > "$PACKET_ROOT/server-runs-after.json"
SERVER_RUN_ID="$(bind_new_dispatch_run "$PACKET_ROOT/server-runs-before.json" "$PACKET_ROOT/server-runs-after.json" "$SOURCE_RELEASE_HEAD" "$RELEASE_REF" "$DISPATCHED_AT" "repos/EffortlessMetrics/ripr/actions/workflows/release-server-binaries.yml/runs?per_page=100&event=workflow_dispatch")"
wait_for_run_success "$SERVER_RUN_ID"
gh run view "$SERVER_RUN_ID" --repo EffortlessMetrics/ripr --json databaseId,headSha,status,conclusion,url > "$PACKET_ROOT/server-run-receipt.json"
jq -e --arg sha "$SOURCE_RELEASE_HEAD" '.headSha == $sha and .status == "completed" and .conclusion == "success"' "$PACKET_ROOT/server-run-receipt.json" >/dev/null
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; record the published server manifest's raw-byte digest before any extension dispatch
curl -fsSL "https://github.com/EffortlessMetrics/ripr/releases/download/v${VERSION}/ripr-server-manifest-v${VERSION}.json" -o "$PACKET_ROOT/ripr-server-manifest-v${VERSION}.json"
SERVER_MANIFEST_SHA256="$(sha256sum "$PACKET_ROOT/ripr-server-manifest-v${VERSION}.json" | cut -d' ' -f1)"
printf '%s\n' "$SERVER_MANIFEST_SHA256" > "$PACKET_ROOT/server-manifest.sha256"
```

`publish-extension.yml` refuses a dispatch whose run commit is not `source_sha`
or whose `v$VERSION` tag does not peel to it, re-downloads the manifest and
every archive it names and compares them with `manifest_sha256`, requires
`package.json` to equal the version, binds that digest into the VSIX through
`cargo xtask release-bind-server-descriptor`, runs `npm run package` plus
`cargo xtask vscode-verify-vsix`, and attaches one VSIX to the Release before
either marketplace publish. Whichever marketplace dispatch runs first attaches
the VSIX, so it also needs the VSIX-asset item; when VS Code Marketplace is
withheld, skip its three blocks and the Open VSX dispatch becomes that first
dispatch. The second channel's dispatch must pass the attached
VSIX's digest as `vsix_sha256` (recorded below before that dispatch); it
downloads the attached bytes, checks them against it, and each publish job
re-checks their SHA-256 against the Release asset. The workflow cannot prove
equivalence with the swarm qualification archives, whose commit and URLs
differ; that remains outside this control (#3807 R8-F2).

```bash
set -euo pipefail
# [EXTERNAL-PUBLISHING] repo=source; explicit #1470 VSIX-asset and VS Marketplace authorization only
require_authorized "VSIX GitHub asset upload when applicable"
require_authorized "VS Code Marketplace publication"
DISPATCHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
gh api --paginate --slurp "repos/EffortlessMetrics/ripr/actions/workflows/publish-extension.yml/runs?per_page=100&event=workflow_dispatch" | jq '[.[] | .workflow_runs[]]' > "$PACKET_ROOT/extension-runs-before.json"
gh workflow run publish-extension.yml --repo EffortlessMetrics/ripr --ref "$RELEASE_REF" -f version="$VERSION" -f source_sha="$SOURCE_RELEASE_HEAD" -f manifest_sha256="$SERVER_MANIFEST_SHA256" -f publish_vs_marketplace=true -f publish_open_vsx=false
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; paginate and bind the dispatched VS Marketplace run
gh api --paginate --slurp "repos/EffortlessMetrics/ripr/actions/workflows/publish-extension.yml/runs?per_page=100&event=workflow_dispatch" | jq '[.[] | .workflow_runs[]]' > "$PACKET_ROOT/extension-runs-after.json"
EXTENSION_RUN_ID="$(bind_new_dispatch_run "$PACKET_ROOT/extension-runs-before.json" "$PACKET_ROOT/extension-runs-after.json" "$SOURCE_RELEASE_HEAD" "$RELEASE_REF" "$DISPATCHED_AT" "repos/EffortlessMetrics/ripr/actions/workflows/publish-extension.yml/runs?per_page=100&event=workflow_dispatch")"
wait_for_run_success "$EXTENSION_RUN_ID"
gh run view "$EXTENSION_RUN_ID" --repo EffortlessMetrics/ripr --json databaseId,headSha,status,conclusion,url > "$PACKET_ROOT/vs-marketplace-run-receipt.json"
jq -e --arg sha "$SOURCE_RELEASE_HEAD" '.headSha == $sha and .status == "completed" and .conclusion == "success"' "$PACKET_ROOT/vs-marketplace-run-receipt.json" >/dev/null
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; record the digest of the VSIX the first dispatch attached to the Release
gh release download "v${VERSION}" --repo EffortlessMetrics/ripr --pattern "ripr-${VERSION}.vsix" --dir "$PACKET_ROOT"
VSIX_SHA256="$(sha256sum "$PACKET_ROOT/ripr-${VERSION}.vsix" | cut -d' ' -f1)"
printf '%s\n' "$VSIX_SHA256" > "$PACKET_ROOT/vsix.sha256"
```

```bash
set -euo pipefail
# [EXTERNAL-PUBLISHING] repo=source; explicit #1470 Open VSX authorization only
require_authorized "Open VSX publication"
# With no earlier dispatch this one attaches the VSIX, which is its own #1470 item.
test -n "${VSIX_SHA256:-}" || require_authorized "VSIX GitHub asset upload when applicable"
DISPATCHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
gh api --paginate --slurp "repos/EffortlessMetrics/ripr/actions/workflows/publish-extension.yml/runs?per_page=100&event=workflow_dispatch" | jq '[.[] | .workflow_runs[]]' > "$PACKET_ROOT/extension-runs-open-vsx-before.json"
gh workflow run publish-extension.yml --repo EffortlessMetrics/ripr --ref "$RELEASE_REF" -f version="$VERSION" -f source_sha="$SOURCE_RELEASE_HEAD" -f manifest_sha256="$SERVER_MANIFEST_SHA256" -f vsix_sha256="${VSIX_SHA256:-}" -f publish_vs_marketplace=false -f publish_open_vsx=true
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; paginate and bind the dispatched Open VSX run
gh api --paginate --slurp "repos/EffortlessMetrics/ripr/actions/workflows/publish-extension.yml/runs?per_page=100&event=workflow_dispatch" | jq '[.[] | .workflow_runs[]]' > "$PACKET_ROOT/extension-runs-open-vsx-after.json"
OPENVSX_RUN_ID="$(bind_new_dispatch_run "$PACKET_ROOT/extension-runs-open-vsx-before.json" "$PACKET_ROOT/extension-runs-open-vsx-after.json" "$SOURCE_RELEASE_HEAD" "$RELEASE_REF" "$DISPATCHED_AT" "repos/EffortlessMetrics/ripr/actions/workflows/publish-extension.yml/runs?per_page=100&event=workflow_dispatch")"
wait_for_run_success "$OPENVSX_RUN_ID"
gh run view "$OPENVSX_RUN_ID" --repo EffortlessMetrics/ripr --json databaseId,headSha,status,conclusion,url > "$PACKET_ROOT/open-vsx-run-receipt.json"
jq -e --arg sha "$SOURCE_RELEASE_HEAD" '.headSha == $sha and .status == "completed" and .conclusion == "success"' "$PACKET_ROOT/open-vsx-run-receipt.json" >/dev/null
```

After partial publication, stop later channels, record public state, and obtain
a corrective-release decision through #1470. A failed marketplace does not
invalidate a verified crate, tag, and server release. Never republish a crate
version, delete or retarget a public tag, or claim a complete release.

A withheld or failed marketplace leaves no run receipt. Do not fabricate one.
Record the channel's state as an owner-backed disposition instead, in
`$PACKET_ROOT/vs-marketplace-disposition.json` or
`$PACKET_ROOT/open-vsx-disposition.json`:

```json
{
  "channel": "vs_marketplace",
  "state": "withheld",
  "source_release_head": "<40-hex SOURCE_RELEASE_HEAD>",
  "authority_url": "https://github.com/EffortlessMetrics/ripr/issues/1470#issuecomment-<id>"
}
```

The Open VSX file is the same with `"channel": "open_vsx"`. `state` is
`withheld` (the #1470 record says `not_authorized`, or the channel was never
dispatched) or `failed` (dispatched, the admitted bytes are not what the
channel serves, and #1470 carries the corrective decision). `authority_url` is
the owner's #1470 comment that says so; section 7 checks its form and does not
fetch it.

A disposition is the terminal outcome and wins over a workflow result. The
extension workflow can succeed without publishing the admitted bytes, so a
`failed` disposition may sit beside a run receipt; section 7 then records the
failed row and keeps the run under `workflow_run` as supporting evidence. A
`withheld` disposition beside a run receipt is contradictory and stops. With no
disposition, the marketplace row is its successful run receipt. A marketplace
with neither file stops. K still waits for the terminal `ripr#1481` channel
matrix.

### npm and PyPI

The source repository also carries `publish-npm.yml` (environment `npm`,
`@effortlessmetrics/ripr` under the `next` dist-tag) and `publish-pypi.yml`
(environment `pypi`, `ripr-rs`). Both are dispatched from `main`, not from the
release tag, and both admit **prereleases only** (`docs/NPM_BOOTSTRAP.md` and
`docs/PYPI_BOOTSTRAP.md` in the source repository). Neither can publish a
stable `0.11.0` until its admission contract is extended and stable authority
is granted, so neither has a template in this section and a stable 0.11.0
release does not claim an npm or PyPI install route. Do not rename a prerelease artifact into
a stable one. Their environments are separate operator setup from the
`vs-marketplace-publish` and `open-vsx-publish` environments the extension
workflow needs; the npm environment was absent at the 2026-09-30 inspection.

## 7. Construct and transport K after public verification

Freeze exact post-publication inputs:

```text
SWARM_BEFORE        = exact swarm main before back-sync
SOURCE_RELEASE_HEAD = exact released source main
BACK_SYNC_TREE      = reviewed tree retaining swarm authority
K.parent[0]         = SWARM_BEFORE
K.parent[1]         = SOURCE_RELEASE_HEAD
```

Before leaving the source publication phase, write the frozen values once and
never replace them with a later floating-head lookup:

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; persist frozen K inputs for later guards
test ! -e "$PACKET_ROOT/SWARM_BEFORE" && test ! -e "$PACKET_ROOT/SOURCE_RELEASE_HEAD"
printf '%s\n' "$SWARM_PARENT" > "$PACKET_ROOT/SWARM_BEFORE"
printf '%s\n' "$SOURCE_RELEASE_HEAD" > "$PACKET_ROOT/SOURCE_RELEASE_HEAD"
```

The exact-K contract is [`BACK_SYNC_VERIFIER.md`](BACK_SYNC_VERIFIER.md),
[`RIPR-SPEC-0149`](specs/RIPR-SPEC-0149-back-sync-verifier.md), and
[#3100](https://github.com/EffortlessMetrics/ripr-swarm/issues/3100), merged
at [`4589b85a`](https://github.com/EffortlessMetrics/ripr-swarm/commit/4589b85a8f63529c4838abe5ee54a78d28d989c7). Use its verifier and receipt;
this runbook does not redefine its semantics.

```bash
set -euo pipefail
# [READ-ONLY] repo=swarm+source; freeze K pair and pre-transport heads
test -s "$PACKET_ROOT/SWARM_BEFORE" && test -s "$PACKET_ROOT/SOURCE_RELEASE_HEAD"
SWARM_BEFORE="$(tr -d '\r\n' < "$PACKET_ROOT/SWARM_BEFORE")"
SOURCE_RELEASE_HEAD="$(tr -d '\r\n' < "$PACKET_ROOT/SOURCE_RELEASE_HEAD")"
CURRENT_SWARM_HEAD="$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)"
CURRENT_SOURCE_HEAD="$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)"
test "$CURRENT_SWARM_HEAD" = "$SWARM_BEFORE"
test "$CURRENT_SOURCE_HEAD" = "$SOURCE_RELEASE_HEAD"
test "$(git -C "$SWARM_ROOT" rev-parse "$SWARM_BEFORE^{commit}")" = "$SWARM_BEFORE"
test "$(git -C "$SOURCE_ROOT" rev-parse "$SOURCE_RELEASE_HEAD^{commit}")" = "$SOURCE_RELEASE_HEAD"
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=swarm back-sync checkout; construct reviewed K
assert_live_pin_guard
git -C "$SWARM_ROOT" fetch "$SOURCE_ORIGIN" "$SOURCE_RELEASE_HEAD"
test "$(git -C "$SWARM_ROOT" rev-parse "$SOURCE_RELEASE_HEAD^{commit}")" = "$SOURCE_RELEASE_HEAD"
git -C "$SWARM_ROOT" switch -c "back-sync/${VERSION}" "$SWARM_BEFORE"
git -C "$SWARM_ROOT" merge --no-ff --no-commit "$SOURCE_RELEASE_HEAD"
# Retain swarm checks/runners/settings/authority; import release receipts only.
git -C "$SWARM_ROOT" commit -m "sync: back-sync released ${VERSION} from ripr"
K="$(git -C "$SWARM_ROOT" rev-parse HEAD)"
BACK_SYNC_TREE="${BACK_SYNC_TREE:?set to the separately reviewed back-sync tree SHA}"
test "$(git -C "$SWARM_ROOT" rev-parse "$K^{tree}")" = "$BACK_SYNC_TREE"
```

After K, BACK_SYNC_TREE, and a run receipt or owner-backed disposition for
every channel are available,
produce the exact release-receipt schema consumed by the K verifier. Do this
before either verifier invocation; its semantics remain owned by
[`BACK_SYNC_VERIFIER.md`](BACK_SYNC_VERIFIER.md).

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; produce verifier-bound publication receipt
run_receipt_row() {
  jq -e -c --arg sha "$SOURCE_RELEASE_HEAD" 'select(.headSha == $sha and .status == "completed" and .conclusion == "success")' "$1"
}
marketplace_row() {
  # $1 = packet file stem, $2 = channel key. An owner-backed disposition is
  # the terminal outcome and wins over a workflow result: a run can succeed
  # without publishing the admitted bytes. A missing row is a stop.
  local run="$PACKET_ROOT/$1-run-receipt.json" disposition="$PACKET_ROOT/$1-disposition.json" row
  if test -e "$disposition"; then
    # Explicit returns: errexit does not reach a command substitution.
    row="$(jq -e -c --arg channel "$2" --arg sha "$SOURCE_RELEASE_HEAD" 'select((keys | sort) == ["authority_url", "channel", "source_release_head", "state"] and .channel == $channel and .source_release_head == $sha and (.state == "withheld" or .state == "failed") and (.authority_url | type == "string" and test("^https://github\\.com/EffortlessMetrics/ripr/issues/1470#issuecomment-[0-9]+$")))' "$disposition")" || return 1
    if test -e "$run"; then
      # A withheld channel was never dispatched, so it has no run. A failed
      # one keeps its run, whatever its conclusion, as supporting evidence.
      jq -e -c --argjson row "$row" --arg sha "$SOURCE_RELEASE_HEAD" 'select($row.state == "failed" and .headSha == $sha) | $row + {workflow_run: .}' "$run" || return 1
    else
      printf '%s\n' "$row"
    fi
  else
    run_receipt_row "$run"
  fi
}
SERVER_ROW="$(run_receipt_row "$PACKET_ROOT/server-run-receipt.json")"
MARKETPLACE_ROW="$(marketplace_row vs-marketplace vs_marketplace)"
OPEN_VSX_ROW="$(marketplace_row open-vsx open_vsx)"
jq -n --arg version "$VERSION" --arg source_release_head "$SOURCE_RELEASE_HEAD" --arg join "$K" --arg tree "$BACK_SYNC_TREE" --arg source_release_tag "v${VERSION}" --argjson server "$SERVER_ROW" --argjson marketplace "$MARKETPLACE_ROW" --argjson open_vsx "$OPEN_VSX_ROW" '{schema_version: 1, version: $version, source_release_head: $source_release_head, join: $join, tree: $tree, source_release_tag: $source_release_tag, channels: {server_binaries: $server, vs_marketplace: $marketplace, open_vsx: $open_vsx}}' > "$PUBLICATION_RECEIPT"
jq -e --arg version "$VERSION" --arg source_release_head "$SOURCE_RELEASE_HEAD" --arg join "$K" --arg tree "$BACK_SYNC_TREE" '.version == $version and .source_release_head == $source_release_head and .join == $join and .tree == $tree and .source_release_tag == ("v" + $version) and (.channels | keys | sort) == ["open_vsx", "server_binaries", "vs_marketplace"] and (.channels.server_binaries.conclusion == "success") and ([.channels.vs_marketplace, .channels.open_vsx] | all(.conclusion == "success" or .state == "withheld" or .state == "failed"))' "$PUBLICATION_RECEIPT" >/dev/null
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=swarm; write policy-before evidence
gh api repos/EffortlessMetrics/ripr-swarm/branches/main/protection > "$POLICY_BEFORE"
gh api --paginate --slurp repos/EffortlessMetrics/ripr-swarm/rulesets > "$PACKET_ROOT/policy-rulesets-before.json"
ACTIVE_RULESET_IDS="$(jq -r '.[][] | select(.enforcement == "active" and .target == "branch") | .id' "$PACKET_ROOT/policy-rulesets-before.json")"
for ruleset_id in $ACTIVE_RULESET_IDS; do
  gh api "repos/EffortlessMetrics/ripr-swarm/rulesets/${ruleset_id}" > "$PACKET_ROOT/ruleset-${ruleset_id}-before.json"
done
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=swarm+packet; fetch and verify before-state and owner authorization
jq -e '.allow_force_pushes.enabled == false and .allow_deletions.enabled == false' "$POLICY_BEFORE" >/dev/null
test -s "$POLICY_APPROVAL"
POLICY_OWNER_LOGIN="${POLICY_OWNER_LOGIN:?set the expected approving owner login}"
test "$(gh api user --jq .login)" = "$POLICY_OWNER_LOGIN"
POLICY_BEFORE_SHA="$(sha256sum "$POLICY_BEFORE" | awk '{print $1}')"
POLICY_VALIDATED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
POLICY_NOW_EPOCH="$(date -u -d "$POLICY_VALIDATED_AT" +%s)"
POLICY_EXPIRES_AT="$(jq -r '.expires_at // empty' "$POLICY_APPROVAL")"
printf '%s\n' "$POLICY_EXPIRES_AT" | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(Z|[+-][0-9]{2}:[0-9]{2})$'
printf '%s\n' "$POLICY_EXPIRES_AT" | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$'
POLICY_EXPIRES_EPOCH="$(date -u -d "$POLICY_EXPIRES_AT" +%s 2>/dev/null || true)"
test "$(date -u -d "@$POLICY_EXPIRES_EPOCH" +%Y-%m-%dT%H:%M:%SZ)" = "$POLICY_EXPIRES_AT"
test -n "$POLICY_EXPIRES_EPOCH"
test "$POLICY_EXPIRES_EPOCH" -gt "$POLICY_NOW_EPOCH"
jq -n --arg validated_at "$POLICY_VALIDATED_AT" --arg expires_at "$POLICY_EXPIRES_AT" --argjson validated_epoch "$POLICY_NOW_EPOCH" --argjson expires_epoch "$POLICY_EXPIRES_EPOCH" '{schema_version: 1, validated_at: $validated_at, expires_at: $expires_at, validated_epoch: $validated_epoch, expires_epoch: $expires_epoch}' > "$PACKET_ROOT/policy-approval-validation.json"
jq -e --arg owner "$POLICY_OWNER_LOGIN" --arg version "$VERSION" --arg swarm "$SWARM_BEFORE" --arg source "$SOURCE_RELEASE_HEAD" --arg before_sha "$POLICY_BEFORE_SHA" '
  .schema_version == 1 and .approval_issue == 3104 and
  .owner.login == $owner and .approved_for == "ancestry-preserving-back-sync" and
  .version == $version and .swarm_before == $swarm and
  .source_release_head == $source and .before_sha256 == $before_sha and
  (.protected_layers | type == "array") and
  (.expires_at | type == "string") and (.approved_at | type == "string")
' "$POLICY_APPROVAL" >/dev/null
for ruleset_id in $ACTIVE_RULESET_IDS; do
  jq -e --arg id "$ruleset_id" '.protected_layers | index("ruleset:" + $id) != null' "$POLICY_APPROVAL" >/dev/null
done
git -C "$SWARM_ROOT" fetch origin main
test "$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)" = "$SWARM_BEFORE"
```

The following is the copyable, owner-authorized external-setting operation.
It is not run by this documentation PR. It preserves the current branch
protection payload while changing only `required_linear_history` for the
ancestry-preserving merge; if linear history is already disabled, it records
the before response as the effective exception without changing settings.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; build exact branch-protection exception request
jq '{
  required_status_checks: (if .required_status_checks == null then null else {strict: .required_status_checks.strict, contexts: .required_status_checks.contexts, checks: .required_status_checks.checks} end),
  enforce_admins: .enforce_admins.enabled,
  required_pull_request_reviews: null,
  restrictions: (if .restrictions == null then null else {users: .restrictions.users, teams: .restrictions.teams, apps: .restrictions.apps} end),
  required_linear_history: false,
  allow_force_pushes: .allow_force_pushes.enabled,
  allow_deletions: .allow_deletions.enabled,
  block_creations: .block_creations.enabled,
  required_conversation_resolution: .required_conversation_resolution.enabled,
  lock_branch: .lock_branch.enabled,
  allow_fork_syncing: .allow_fork_syncing.enabled
}' "$POLICY_BEFORE" > "$POLICY_EXCEPTION_REQUEST"
```

```bash
set -euo pipefail
# [EXTERNAL-PUBLISHING] repo=swarm branch protection; owner-approved setting mutation only
if test "$(jq -r '.required_linear_history.enabled' "$POLICY_BEFORE")" = true; then
  gh api --method PUT repos/EffortlessMetrics/ripr-swarm/branches/main/protection --input "$POLICY_EXCEPTION_REQUEST" > "$POLICY_EXCEPTION"
else
  cp "$POLICY_BEFORE" "$POLICY_EXCEPTION"
fi
for ruleset_id in $ACTIVE_RULESET_IDS; do
  RULESET_EXCEPTION_REQUEST="$PACKET_ROOT/ruleset-${ruleset_id}-exception-request.json"
  RULESET_BEFORE="$PACKET_ROOT/ruleset-${ruleset_id}-before.json"
  jq '(.rules // []) as $rules | ($rules | map(select(.type == "pull_request" or .type == "required_linear_history"))) as $approved | if ($approved | length) > 0 then .rules = ($rules | map(select(.type != "pull_request" and .type != "required_linear_history"))) else . end' "$RULESET_BEFORE" > "$RULESET_EXCEPTION_REQUEST"
  jq -e --slurpfile before "$RULESET_BEFORE" '((del(.rules) == ($before[0] | del(.rules))) and ((($before[0].rules // []) | map(select(.type == "pull_request" or .type == "required_linear_history")) | length) > 0) and ((.rules // []) | all(.type != "pull_request" and .type != "required_linear_history"))) or ((. == $before[0]) and (($before[0].rules // []) | map(select(.type == "pull_request" or .type == "required_linear_history")) | length) == 0)' "$RULESET_EXCEPTION_REQUEST" >/dev/null
  if test "$(jq '[.rules[]? | select(.type == "pull_request" or .type == "required_linear_history")] | length' "$RULESET_BEFORE")" -gt 0; then
    gh api --method PUT "repos/EffortlessMetrics/ripr-swarm/rulesets/${ruleset_id}" --input "$RULESET_EXCEPTION_REQUEST" > "$PACKET_ROOT/ruleset-${ruleset_id}-exception.json"
  else
    cp "$RULESET_BEFORE" "$PACKET_ROOT/ruleset-${ruleset_id}-exception.json"
    printf '%s\n' "no approved K-blocking rules present; no ruleset mutation" > "$PACKET_ROOT/ruleset-${ruleset_id}-no-mutation.txt"
  fi
  jq -e --slurpfile before "$RULESET_BEFORE" '((del(.rules) == ($before[0] | del(.rules))) and ((.rules // []) == (($before[0].rules // []) | map(select(.type != "pull_request" and .type != "required_linear_history"))))) or (. == $before[0])' "$PACKET_ROOT/ruleset-${ruleset_id}-exception.json" >/dev/null
done
```

Each active ruleset is reviewed as a separate protection layer. A ruleset may
be listed as unchanged in the owner receipt, but a layer that blocks the exact
K direct update requires its own owner-created PUT payload and temporary
response. The branch exception clears only the main-branch pull-request review
requirement needed for this direct K update; required status checks, force-push
protection, deletion protection, and every unrelated rule remain enabled. No
unrelated rule may be weakened.

```bash
set -euo pipefail
# [READ-ONLY] repo=swarm policy; verify effective exception before K transport
jq -e '.required_linear_history.enabled == false and .allow_force_pushes.enabled == false and .allow_deletions.enabled == false' "$POLICY_EXCEPTION" >/dev/null
jq -e '.required_pull_request_reviews == null' "$POLICY_EXCEPTION" >/dev/null
for ruleset_id in $ACTIVE_RULESET_IDS; do
  test -s "$PACKET_ROOT/ruleset-${ruleset_id}-exception.json"
done
```

Restore the recorded before-state immediately after transport. Never
force-push, squash, rebase, or merge unrelated work.

`POLICY_APPROVAL` is an owner-provided JSON receipt with this minimum schema;
the owner login is also checked against the authenticated `gh api user`:

```json
{
  "schema_version": 1,
  "approval_issue": 3104,
  "owner": {"login": "release-owner"},
  "approved_for": "ancestry-preserving-back-sync",
  "version": "0.11.0",
  "swarm_before": "<40-hex-sha>",
  "source_release_head": "<40-hex-sha>",
  "before_sha256": "<64-hex-sha256>",
  "protected_layers": ["branch:main", "ruleset:<id>"],
  "approved_at": "<RFC3339>",
  "expires_at": "<RFC3339>"
}
```

```bash
set -euo pipefail
# [EXTERNAL-PUBLISHING] repo=swarm remote/policy; owner-approved K transport only
git -C "$SWARM_ROOT" fetch origin main
test "$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)" = "$SWARM_BEFORE"
git -C "$SWARM_ROOT" push origin "back-sync/${VERSION}:refs/heads/main"
for ruleset_id in $ACTIVE_RULESET_IDS; do
  RULESET_BEFORE_REQUEST="$PACKET_ROOT/ruleset-${ruleset_id}-before-request.json"
  test -s "$RULESET_BEFORE_REQUEST"
  gh api --method PUT "repos/EffortlessMetrics/ripr-swarm/rulesets/${ruleset_id}" --input "$RULESET_BEFORE_REQUEST" > "$PACKET_ROOT/ruleset-${ruleset_id}-after.json"
done
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=swarm remote; fetch and bind swarm main to K after transport
git -C "$SWARM_ROOT" fetch origin main
test "$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)" = "$K"
```

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=packet; build exact before-state restoration request
jq '{required_status_checks: (if .required_status_checks == null then null else {strict: .required_status_checks.strict, contexts: .required_status_checks.contexts, checks: .required_status_checks.checks} end), enforce_admins: .enforce_admins.enabled, required_pull_request_reviews: (if .required_pull_request_reviews == null then null else {dismiss_stale_reviews: .required_pull_request_reviews.dismiss_stale_reviews, require_code_owner_reviews: .required_pull_request_reviews.require_code_owner_reviews, required_approving_review_count: .required_pull_request_reviews.required_approving_review_count, require_last_push_approval: .required_pull_request_reviews.require_last_push_approval} end), restrictions: (if .restrictions == null then null else {users: .restrictions.users, teams: .restrictions.teams, apps: .restrictions.apps} end), required_linear_history: .required_linear_history.enabled, allow_force_pushes: .allow_force_pushes.enabled, allow_deletions: .allow_deletions.enabled, block_creations: .block_creations.enabled, required_conversation_resolution: .required_conversation_resolution.enabled, lock_branch: .lock_branch.enabled, allow_fork_syncing: .allow_fork_syncing.enabled}' "$POLICY_BEFORE" > "$POLICY_RESTORE_REQUEST"
```

```bash
set -euo pipefail
# [EXTERNAL-PUBLISHING] repo=swarm branch protection; restore exact before-state
gh api --method PUT repos/EffortlessMetrics/ripr-swarm/branches/main/protection --input "$POLICY_RESTORE_REQUEST" > "$POLICY_AFTER"
```

```bash
set -euo pipefail
# [READ-ONLY] repo=swarm policy; prove normal enforcement is restored
jq -e --slurpfile before "$POLICY_BEFORE" '(.required_status_checks == $before[0].required_status_checks) and (.enforce_admins == $before[0].enforce_admins) and (.required_pull_request_reviews == $before[0].required_pull_request_reviews) and (.restrictions == $before[0].restrictions) and (.required_linear_history == $before[0].required_linear_history) and (.allow_force_pushes == $before[0].allow_force_pushes) and (.allow_deletions == $before[0].allow_deletions) and (.block_creations == $before[0].block_creations) and (.required_conversation_resolution == $before[0].required_conversation_resolution) and (.lock_branch == $before[0].lock_branch) and (.allow_fork_syncing == $before[0].allow_fork_syncing)' "$POLICY_AFTER" >/dev/null
for ruleset_id in $ACTIVE_RULESET_IDS; do
  jq -S '{name,target,enforcement,conditions,rules}' "$PACKET_ROOT/ruleset-${ruleset_id}-before.json" > "$PACKET_ROOT/ruleset-${ruleset_id}-before-shape.json"
  jq -S '{name,target,enforcement,conditions,rules}' "$PACKET_ROOT/ruleset-${ruleset_id}-after.json" > "$PACKET_ROOT/ruleset-${ruleset_id}-after-shape.json"
  cmp "$PACKET_ROOT/ruleset-${ruleset_id}-before-shape.json" "$PACKET_ROOT/ruleset-${ruleset_id}-after-shape.json"
done
```

```bash
set -euo pipefail
# [READ-ONLY] repo=swarm+source; verify exact K after transport and restoration
(cd "$SWARM_ROOT" && cargo xtask back-sync verify \
  --swarm-before "$SWARM_BEFORE" --source-release-head "$SOURCE_RELEASE_HEAD" \
  --source-release-tag "v${VERSION}" --join "$K" --tree "$BACK_SYNC_TREE" \
  --swarm-repo "$SWARM_ROOT" --source-repo "$SOURCE_ROOT" --version "$VERSION" \
  --release-receipt "$PUBLICATION_RECEIPT" --policy-before "$POLICY_BEFORE" \
  --policy-exception "$POLICY_EXCEPTION" --policy-after "$POLICY_AFTER" \
  --swarm-main "$K" --source-main "$SOURCE_RELEASE_HEAD" \
  --out "$PACKET_ROOT/back-sync-after")
```

J is `[SOURCE_PARENT, SWARM_PARENT]`; K is `[SWARM_BEFORE,
SOURCE_RELEASE_HEAD]`. Neither proves release correctness, artifact adequacy,
publication success, or semantic equivalence. Source publication workflows and
settings remain ancestry evidence and never become swarm authority.

## 8. Re-baseline and clean up

Only after K is reachable from swarm main, policy is restored, and the
publication receipt is complete may development resume. Re-baseline `0.11.1`
from exact K, not a candidate or floating branch.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=swarm; fetch and reopen development only from published K
git -C "$SWARM_ROOT" fetch origin main
test "$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)" = "$K"
git -C "$SWARM_ROOT" log --oneline --decorate -n 5 "$K"
```

Remove only clean worktrees, local branches, and scratch files created by this
transaction. Keep shared Cargo/npm caches and historical receipts.

```bash
set -euo pipefail
# [LOCAL-MUTATING] repo=swarm+source; lane-created clean surfaces only
test -z "$(git -C "$QUAL_ROOT" status --short)"
git -C "$SWARM_ROOT" worktree remove "$QUAL_ROOT"
git -C "$SWARM_ROOT" switch --detach "$K"
git -C "$SWARM_ROOT" branch -d "back-sync/${VERSION}"
git -C "$SOURCE_ROOT" switch --detach "$SOURCE_RELEASE_HEAD"
git -C "$SOURCE_ROOT" branch -d "release/${VERSION}" 2>/dev/null || true
```

## Failure recovery and rollback

- Before pin: fix premise or queue disposition; no release claim exists.
- After pin: changed input supersedes the receipt; retain it and repin only
  under section 2's rule.
- J conflict/verifier failure: regenerate preflight and rebuild J; never append
  a repair commit.
- Source metadata/artifact failure: do not tag or publish; refresh affected
  receipts after a separate source fix.
- Partial publication: stop later channels, record public state, and obtain a
  corrective-release decision through #1470; do not rewrite J.
- K expected-head/policy failure: restore policy, retain untransported K, and
  return to the owner-approved transport decision; never force-push.
- After K: do not rewrite it to repair a receipt; a corrective source release
  requires a new authorized transaction if swarm must receive it.

The safe rollback for this documentation lane is to revert its PR. This
runbook never authorizes direct `origin/main` mutation, force pushes, deletion
of ambiguous work, or deletion of historical evidence.
