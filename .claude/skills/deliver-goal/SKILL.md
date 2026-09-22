---
name: deliver-goal
description: Carry a high-level repository outcome through the distinct PR-sized claims needed to satisfy it. Use when the user states an end state rather than one already-scoped issue or PR.
---

# Useful result

The user's original end state remains the parent authority, the selected release or product denominator stays visible, every required claim is delivered, in flight, externally blocked, or honestly not established, and the goal is closed only when the parent end state exists.

# Goal contract markers

- `goal_contract:parent_end_state`
- `goal_contract:progress_denominator`
- `goal_contract:local_work_not_delivery`
- `goal_contract:waiting_lane_not_global_blocker`
- `goal_contract:subgoal_does_not_close_parent`
- `goal_contract:primary_sources_before_summary`

# Authority and rehydration law

At session start, after compaction, or after a handoff, reconstruct the goal from this order:

1. the current user instruction;
2. root `CLAUDE.md` and the applicable `.claude/skills/**` procedure;
3. live repository source plus current GitHub issues, PRs, checks, and retained artifacts;
4. prior summaries, subagent reports, progress recaps, and local notes.

Lower-ranked material is a lead, not authority. It may not create a permission boundary, user ruling, exact count, object identity, completion state, or release disposition. When a summary conflicts with a primary source, discard the summary claim and continue from the primary source.

Preserve the user's original parent end state. A session, model, agent, worktree, issue slice, or current PR set does not create a smaller ownership boundary such as "my share" unless the user explicitly scoped the goal that way. A runtime `/goal` is an execution aid, not permission to replace the parent outcome with a convenient checklist.

Write runtime goal text as a durable end condition. Current PR numbers, temporary issue states, current worker ownership, and the immediate queue belong in progress fields or the live graph, not in the objective. Do not make a moving pointer part of the definition of done.

If a runtime goal is stale or contradicts the live instruction, replace it with a successor objective that preserves the parent end state and continue. Do not ask the user to restate a goal already recoverable from current authority merely because the runtime cannot edit a completed historical goal.

# Procedure

1. Preserve the user's goal verbatim and identify the parent end state. Do not replace it with the first plausible issue, current PR batch, or reconciliation task.
2. Record the current interpretation:
   - desired end state;
   - constraints and maturity boundary;
   - non-goals;
   - material assumptions;
   - unresolved owner decisions;
   - acceptance predicates;
   - current phase and the transition that ends it.
3. Reconstruct current truth from current `main`, GitHub issues and PRs, required checks, controlling specs/ADRs/policies, retained receipts, and the owning production path.
4. Reconcile existing work by claim identity:
   - resume an equivalent existing PR;
   - reuse or update an existing issue;
   - respect an explicit prerequisite;
   - distinguish already-landed work from open implementation;
   - do not infer ownership from nearby files, crates, or symbols.
5. Fix the progress denominator to the complete parent acceptance set. Add newly discovered required predicates; never shrink the denominator merely because the current session touched a smaller subset.
6. Select one coherent ready claim whose delivery would move a parent predicate. Use `deliver-pr`.
7. When a PR, build, subagent, or worker reaches a remote-owned or long-running state, leave that lane in flight and advance another distinct ready claim when useful. One waiting lane is not a global blocker.
8. Revisit in-flight work only after a material transition: a finding, failed required check, changed head, concrete conflict, changed prerequisite, merge, closure, or completed retained artifact.
9. After every merge or deliberate closure, reconcile the issue, remaining acceptance, parent goal, generated evidence, and next ready claim. A merged PR with an incomplete parent goal must continue through this loop.
10. Re-evaluate every parent predicate as one of:
    - `pass`;
    - `failed`;
    - `limited`;
    - `not_applicable`;
    - `not_established`.
11. Stop only when the parent goal is satisfied, every remaining required claim shares a genuine external blocker, a material non-derivable owner decision remains, or the result is honestly not established.

# Progress and status law

Use repository delivery state, not effort, token count, local checklist completion, or conversational momentum:

```text
local edit or local commit     unpublished candidate; zero repository delivery
open PR                        in flight
merged PR                      implementation landed
terminal issue acceptance      claim delivered
immutable candidate receipt    release membership selected
candidate qualification        candidate qualified
history-preserving source sync source integrated
ship packet / authorization    release decision
public tag and publication     release delivered
independent public verification release verified
```

Never promote one state into another. A green readiness lens, local test run, or merged leaf cannot become whole-release completion by prose.

Progress is:

```text
terminal parent predicates / complete selected parent denominator
```

An atomic subgoal may reach 100% without moving the parent to 100%. When reporting such a result, state the parent status and next parent transition in the headline. Do not complete the parent goal because one local queue, "session share," or temporary checklist is exhausted.

Every status report names:

```text
object
authority
exact identity
evidence observed
current state
unknowns
non-claim
next transition
```

# Decision law

The existence of several reasonable engineering choices does not require escalation. Research the governing sources, choose the strongest reversible option, document the rationale, and proceed. Return `NEEDS_OWNER_DECISION` only when materially different viable outcomes remain after safe research and reversible engineering are exhausted.

Routine reversible repository delivery inside the selected goal is not an owner decision: commit coherent candidates, push ordinary branches, open or update PRs, address review and CI, use normal protected merge, and clean lane-created branches/worktrees without pausing for permission. Separate authorization remains required for destructive shared-history changes, repository settings or secrets, public tags/releases/publication/signing/credentials, durable-evidence deletion, or work outside the selected goal.

# Release-scope law

When a release is pinned, the reviewed immutable pin receipt is the sole membership authority; qualification, source preflight, and finalization consume its exact ref, ancestry, ordered SHA digest, PR dispositions, and manifests unchanged. Ordinary `main` or swarm movement never repins or changes membership. Repin only after a release-invalidating exact-candidate qualification or source-preflight failure, with an explicit superseding receipt. Never close, draft, lock, relabel, retarget, or otherwise mutate an unrelated PR to freeze that scope; unrelated PRs stay open and may evolve. Later merges do not retarget the pinned release. Close only the selected PR for its own evidence-backed terminal disposition—never close it now to reopen it after release.

# Concurrency law

- Many distinct claims may be in flight.
- One claim normally has one current candidate.
- One writer mutates a candidate branch or worktree at a time.
- Readers, researchers, and reviewers may inspect the candidate when they improve evidence or elapsed time.
- A delegated writer receives one candidate-owned branch/worktree; the root does not mutate it concurrently.
- Do not monitor sibling implementations, reserve files, or build overlap maps.
- Check other work only for the same claim, an explicit prerequisite, a current all-state duplicate search, or a concrete Git/integration conflict.
- Do not poll an unchanged long-running command while other ready graph nodes exist. Use separate worktrees or workers for independent claims and avoid stacking broad Cargo builds on the same constrained host.

# Useful fan-out

Use focused read-only agents only when they change the evidence or context, for example:

- repository and authority mapping;
- external semantic research;
- test-oracle challenge;
- security, privacy, compatibility, or product review.

The root owns synthesis. Subagent reports are leads until verified against artifacts.

# Valid exits

- `GOAL_SATISFIED`
- `GOAL_PARTIAL`
- `GOAL_IN_FLIGHT`
- `EXTERNAL_BLOCKER`
- `NEEDS_OWNER_DECISION`
- `NOT_ESTABLISHED`

A waiting PR or build is normally `GOAL_IN_FLIGHT`, not `EXTERNAL_BLOCKER`. "No more issues found," "my share is complete," or a 100% atomic subgoal is never equivalent to `GOAL_SATISFIED` unless the parent end state actually exists.
