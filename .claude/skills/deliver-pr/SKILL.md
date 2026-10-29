---
name: deliver-pr
description: Carry one coherent issue or claim from current repository truth through proof, implementation, substantive current-head review, merge, and reconciliation. Use for a selected PR-sized lane or an existing PR.
---

# Useful result

One coherent claim has one current candidate, current proof and review for the affected seams, a durable GitHub PR state, and an accurate issue disposition after merge or deliberate closure.

# Route markers

- `review_route:deliver_pr_to_review_pr`

# PR operating contract markers

- `pr_contract:current_claim_search`
- `pr_contract:duplicate_check_before_conflict`
- `pr_contract:behind_only_no_restack`
- `pr_contract:routine_repo_writes`
- `pr_contract:ordinary_squash_merge`
- `pr_contract:local_commit_not_delivery`

# Procedure

1. Hydrate the selected claim from the current user instruction, root `CLAUDE.md`, the issue, governing artifacts, current source, live GitHub state, and any existing PR. Prior summaries and subagent reports are leads until verified.
2. Search current source, all-state PRs, recently merged PRs, and the controlling issue for an equivalent implementation before creating a branch, and read the issue's claim comments. Reuse the current candidate when one exists. Post or update the issue claim comment per `prepare-issue` (Issue claim comment) before the first push.
3. Identify the earliest missing or stale judgment:
   - premise or issue quality;
   - proof design;
   - implementation;
   - test hardening;
   - simplification;
   - candidate challenge;
   - substantive current-head review;
   - review-comment repair;
   - integration proof;
   - reconciliation.
4. Enter at that point. Do not recreate completed ceremony merely because this session arrived later.
5. If the issue is missing or materially wrong, use `prepare-issue` and continue.
6. If proof is absent or self-confirming, use `prepare-proof` and continue.
7. Build or repair the one current candidate with `build-candidate` and materialize an exact committed head.
8. Perform directed `review-pr` on the exact head in the owning session; optional subagents and advisory providers add evidence without mandatory identity separation. Before publication, retain unavailable remote checks, artifacts and actual required review as `REVIEW_INCOMPLETE`; do not convert them to pass.
9. Inspect live event triggers before branch or PR writes: ordinary publication must start zero development CI workflows, including wrappers. Use only a route consistent with current authority as described in [merge flow](../../../docs/MERGE_FLOW.md); if unavailable, preserve the exact contribution and record its cutover owner rather than dispatching legacy CI. Use `finish-pr` to publish or resume the exact candidate when no equivalent PR already exists. Committing, pushing an ordinary branch, opening or updating the PR, and repairing it are routine delivery inside the selected claim; do not pause for owner permission. Immediately before pushing to a candidate branch, re-read the PR head (`gh pr view --json headRefOid` or equivalent) and compare it with the expected candidate SHA; see Candidate law for an externally advanced head.
10. Immediately before publication, before resolving a conflict, and before replacing a supposedly stale candidate, repeat the claim-identity search against current `main`, open PRs, and recently merged PRs. If equivalent work landed, stop conflict repair, preserve any unique residual, and close or disposition the duplicate instead of manufacturing a rival PR. Search once more right after opening the PR; a competing PR for the same claim is resolved by the competing-PR rule, not by creation order alone. Keep the claim comment's `State:` line current as the candidate advances.
11. Re-enter `review-pr` on the exact published PR head after remote evidence is available. A green check set, empty thread list, or unavailable reviewer does not establish substantive review.
12. Route `REPAIR_REQUIRED` back through the same candidate, then refresh only the affected proof and review dimensions. Before repairing a PR gate failure, reproduce the failing signal on current `origin/main` and record the classification (`inherited`, `candidate`, or `not established`) in the PR body or owned comment; do not repair before that classification exists, and run `cargo xtask ci-policy` locally for gate-relevant repairs instead of spending a hosted cycle. Local `precommit` is not equivalent to `ci-policy` coverage.
13. Immediately before entering `finish-pr` merge convergence, re-read the PR head (`gh pr view --json headRefOid` or equivalent) and compare it with the reviewed candidate SHA. Only a current `REVIEW_READY` PR head may enter `finish-pr` conditional admission or merge convergence; a changed head needs affected-dimension re-verification. For native queue admission, name future G qualification as pending rather than circularly requiring it before enqueue. Explicit incomplete or blocking review states remain draft or durably in flight.
14. After merge or deliberate closure, verify current `main`, update delivered versus remaining acceptance, update parents, and release the candidate worktree.

# Continuation handoff

At a phase change or bounded worker return, recover the current user scope and
surviving limits from their authoritative sources. Before retiring a phase-only
restriction, compare the instruction that imposed it with the current user
instruction or accepted delegation authorizing the next action. Retain that
source and scope in the existing continuation record. A prior lane label,
third-party encouragement, summary or elapsed time is not new authority. Retire
only obsolete task-local restrictions; actual user limits and tool, service or
security denials remain binding. Routine authorized repairs, factual comments and
finish-through need no parent payload approval or renewed phase lease.

A bounded worker's phase completion returns evidence and the next obligation,
not parent completion. The delivery owner verifies the handback and takes the
next useful authorized transition. Preserve that useful owning session through
an ordinary in-window CI wait; record exact native request/run/attempt and the
decision-changing event without tight polling or a separate resident watcher.

At real interruption, update the existing
[PR-native continuation record](../../../docs/PR_AUTOMATION.md#pr-native-continuation-record).
Verify the original owner/task, current candidate and run state before resuming.
A sent message is not an acknowledgment or evidence that work restarted. If the
owner is unavailable, reconcile its actual task/run/branch activity and active
writers, preserve its context and proof, and arrange one explicit transfer through
the existing delivery owner. Elapsed silence alone does not prove inactivity,
and an unacknowledged handoff must not become an indefinite wait. A capacity
restart retains the same authorized claim and evidence; it is not a new rival
candidate or automatic model/provider switch.

If no executable transition remains, name the exact blocker, capable next actor
and observed acknowledgment or missing acknowledgment. Do not imply an unarranged
worker or background watch will continue. Tracking supports delivery and creates
no additional engineering gate or state database.

Use the [paired continuation scenarios](../../../docs/agent-context/delivery-continuation-scenarios.md)
for these boundaries. Scenario decisions do not establish real delivery.

# Candidate law

- One coherent claim normally has one branch, worktree, candidate, and PR.
- A local edit or local commit is an unpublished candidate and counts as zero repository delivery. Delivery begins when the branch and PR are reachable; the claim becomes landed only after protected merge and becomes terminal only after acceptance reconciliation.
- Do not create rival implementations merely to manufacture parallelism.
- Multiple writers may contribute genuinely disjoint pieces only through one integrating candidate owner.
- Do not inspect sibling worktrees or reserve files, crates, APIs, or semantic surfaces.
- A behind-only branch needs no action. Unrelated movement on `main` does not require a rebase, restack, force-push, or broad re-proof.
- A status helper's `BEHIND` observation requests integration judgment, not branch movement. Identify a material reason under the next rule before updating; retain Draft or running-check waits and reuse unaffected proof. Follow the recommendation into per-PR packets and board findings/dispositions so another projection cannot turn the observation into branch-update authority.
- Rebase or update only for an actual content conflict, changed explicit prerequisite, material combined-tree failure, or repository policy that explicitly applies to this candidate.
- Before repairing a conflict, first determine whether the conflicting upstream change already satisfies or supersedes the claim.
- If the published head was externally advanced past the expected candidate SHA, adopt the advanced head as the candidate of record only after diff verification: inspect the new commits and diff and refresh the affected review dimensions. Never force-push over an externally advanced head, and never merge a SHA that differs from the reviewed head without re-verification. This creates no reservation, lease, lock, or sibling-monitoring surface.

# Merge law

Ordinary `ripr-swarm` development PRs use protected squash merge. The squash result is the integration object; the branch's age or distance behind `main` is not a defect by itself. Exact history-preserving source-integration transactions are a separate controlled path and must follow their governing issue rather than this ordinary squash rule.

# Release-scope law

- Pin release membership to the exact immutable head SHA, ancestry, and release manifests.
- Treat the reviewed immutable pin receipt as the sole membership authority: qualification, source preflight, and finalization consume it unchanged. Ordinary `main` or swarm movement never repins or changes membership; repin only after a release-invalidating exact-candidate qualification or source-preflight failure, with an explicit superseding receipt.
- Never close, draft, lock, relabel, retarget, or otherwise mutate an unrelated PR to freeze release scope; unrelated PRs stay open and may evolve.
- A post-pin merge does not retarget the release. Close only this selected PR for its own evidence-backed terminal disposition; never close-now/reopen-after-release.

# Proof and review currentness

Track currentness by dimension rather than treating every SHA change as total invalidation:

- production implementation;
- test stimulus;
- test oracle;
- public claim;
- generated relationships;
- golden drift inherited from a main change;
- conflict resolution;
- integration basis;
- candidate head identity.

Refresh only the dimensions changed by the latest edit. Unrelated movement on `main` invalidates nothing by itself. A current-head review that covered only comments or CI is not a substitute for `review-pr`'s semantic-owner, oracle, contract-parity, platform, and exact-head evidence pass.

# Useful fan-out

The owning session normally inspects authority, tests, correctness, security, compatibility, product behavior and platform semantics through directed review. Optional subagents may inspect bounded questions; observational work does not assign a permanently read-only owner identity. One writer integrates accepted repairs. A delegated writer receives the candidate-owned worktree and is the only mutator until handback. Conflicting reports must be resolved against canonical source and actual behavior before publication.

# Valid exits

- `PR_MERGED`
- `PR_IN_FLIGHT`
- `WAITING_REQUIRED_CHECKS`
- `WAITING_EXTERNAL_REVIEW`
- `WAITING_INTEGRATION_PROOF`
- `PR_CLOSED_WITH_DISPOSITION`
- `REPAIR_REQUIRED`
- `REVIEW_INCOMPLETE`
- `EXTERNAL_BLOCKER`
- `NEEDS_OWNER_DECISION`
- `NOT_ESTABLISHED`

A remote-owned wait yields to the goal loop; it does not keep the root polling unchanged state.
