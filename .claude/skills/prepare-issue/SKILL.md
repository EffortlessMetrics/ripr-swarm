---
name: prepare-issue
description: Research, correct, or create one implementation-ready issue and continue delivery. Use when a claim lacks a current premise, semantic owner, scope, acceptance contract, or dependency boundary.
---

# Result

One issue accurately describes the current problem, governing authority, coherent PR-sized claim, acceptance and negative evidence, dependencies, non-goals, and claim boundary.

# Workflow

1. Preserve the requested outcome and exact referents.
2. Read current `main` and the production consumer before trusting an audit, old issue, or agent report.
3. Search all-state issues, PRs, and relevant Git history for:
   - the same claim;
   - explicit prerequisites;
   - superseding implementation;
   - evidence that the premise is already delivered or stale.
4. Identify the semantic owner and the layer that makes the current decision.
5. Test the strongest counter-read. Keep fact, inference, recommendation, and owner ruling separate.
6. Define one coherent acceptance-and-rollback claim. Do not split by line count, directory, or provider task count.
7. Write acceptance that can fail:
   - intended behavior;
   - discriminating negative or alternate behavior;
   - production-path reachability;
   - currentness and identity where material;
   - explicit claim boundary.
8. Name dependencies and non-goals precisely.
9. Create or update the issue and continue the delivery flow. Do not stop after filing unless planning was the requested deliverable.

# Premise outcomes

- `CURRENT`
- `ALREADY_DELIVERED`
- `PARTIALLY_DELIVERED`
- `DUPLICATE`
- `SUPERSEDED`
- `MIS_SCOPED`
- `NEEDS_FRESH_EVIDENCE`

Partially delivered work stays open with landed and residual acceptance separated.

# Related-work boundary

Search related work by claim identity and explicit dependency, not by touched-file proximity. Material facts needed by another lane belong in an ordinary issue or PR comment; no lane telemetry is required.

## Issue claim comment

The claim is one ordinary comment on the issue, not a lock or state file. `deliver-pr` and `finish-pr` apply these rules. Keep it cheap: full CI runs only at qualification, so a stray duplicate draft costs little. Avoid duplicates where a quick check does it, never block work on ceremony, and hold the one hard rule: only one candidate per claim goes to qualification.

- **Shape.** One comment per claim, edited in place and never re-posted. First line: `Claiming #N: <one-line boundary>`. Add `Branch: <name>` once it is pushed and `PR: #M` once it exists. Add a `State:` line of `candidate`, `qualification-requested`, `qualifying` or `qualified`; it is the next-action signal for anyone reading the issue.
- **Check.** Before claiming and before the first push, read the issue's claim comments as well as searching PRs. Search again right after opening a PR for other PRs that reference the issue.
- **Live claim.** Reconcile the existing claimant's actual task, run, branch and PR activity before changing ownership. An active build, queue wait or acknowledged continuation can preserve useful ownership without a recent comment. Join or review that candidate, or advance independent work; do not create a rival merely to consume capacity. Other people's comments, label changes and takeover notes are not claimant activity.
- **Unresponsive claim.** Roughly three hours without meaningful activity is a reason to check liveness, not automatic takeover authority. Verify the actual task/run/branch state and active writers, attempt acknowledged resumption where supported, and preserve the current candidate, context, findings and proof. If the owner is unavailable, the delivery allocator may arrange one explicit transfer within current user authority and record the successor in the existing claim comment. Do not leave an unacknowledged handoff waiting indefinitely, infer inactivity from silence alone, or edit/push concurrently with an active writer. A claim label or takeover note does not itself establish that transfer.
- **Competing PRs.** If two PRs for one claim exist, only one goes to qualification, chosen on merit (design, proof, review state). Creation order is a tie-break, not the rule. The other contributes any unique residual to the chosen PR and is closed with a link.
- **Release.** When abandoning, edit the claim comment to say so.

# Subagents

Focused read-only subagents may map source authority, GitHub history, external semantics, or adversarial premise risk. The lead Claude context verifies citations and writes one integrated issue.

# Valid outcomes

- `ISSUE_READY`
- `ISSUE_UPDATED`
- `CLAIM_ALREADY_DELIVERED`
- `CLAIM_PARTIAL`
- `EXTERNAL_BLOCKER`
- `NEEDS_OWNER_DECISION`
- `NOT_ESTABLISHED`
