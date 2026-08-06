---
name: deliver-pr
description: Carry one coherent issue or claim from current repository truth through proof, implementation, substantive current-head review, merge, and reconciliation. Use for a selected PR-sized lane or an existing PR.
---

# Useful result

One coherent claim has one current candidate, current proof and review for the affected seams, a durable GitHub PR state, and an accurate issue disposition after merge or deliberate closure.

# Procedure

1. Hydrate the selected claim from the issue, governing artifacts, current source, and any existing PR.
2. Search for an equivalent existing PR before creating a branch. Reuse the current candidate when one exists.
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
7. Build or repair the one current candidate with `build-candidate`.
8. Run `review-pr` on the exact current head before merge convergence. A green check set, empty thread list, or unavailable reviewer does not establish substantive review.
9. Route `REPAIR_REQUIRED` back through the same candidate, then refresh only the affected proof and review dimensions.
10. Publish or resume the PR, then use `finish-pr` only after the current head has a `REVIEW_READY` disposition or an explicit incomplete/blocking state is being carried forward.
11. After merge or deliberate closure, verify current `main`, update delivered versus remaining acceptance, update parents, and release the candidate worktree.

# Candidate law

- One coherent claim normally has one branch, worktree, candidate, and PR.
- Do not create rival implementations merely to manufacture parallelism.
- Multiple writers may contribute genuinely disjoint pieces only through one integrating candidate owner.
- Do not inspect sibling worktrees or reserve files, crates, APIs, or semantic surfaces.
- A behind-only branch needs no action.
- Rebase or update only for an actual conflict, changed explicit prerequisite, material combined-tree failure, or repository policy that applies to this candidate.

# Proof and review currentness

Track currentness by dimension rather than treating every SHA change as total invalidation:

- production implementation;
- test stimulus;
- test oracle;
- public claim;
- generated relationships;
- conflict resolution;
- integration basis;
- candidate head identity.

Refresh only the dimensions changed by the latest edit. Unrelated movement on `main` invalidates nothing by itself. A current-head review that covered only comments or CI is not a substitute for `review-pr`'s semantic-owner, oracle, contract-parity, platform, and exact-head evidence pass.

# Useful fan-out

Focused read-only agents may inspect authority, tests, correctness, security, compatibility, product behavior, or platform semantics. One writer integrates accepted repairs. Conflicting reports must be resolved against canonical source and actual behavior before publication.

# Valid exits

- `PR_MERGED`
- `PR_IN_FLIGHT`
- `AUTO_MERGE_ARMED`
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
