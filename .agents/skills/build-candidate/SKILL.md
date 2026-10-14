---
name: build-candidate
description: Build, harden, simplify, and challenge one coherent candidate. Establish the inherited baseline, discriminate the intended change from wrong behavior, and bind proof to the actual candidate before substantive review.
---

# Useful result

One candidate implements the selected claim, extends its semantic owner, has discriminating proof and an honest baseline/currentness record, and is ready for substantive review. It is not already merge-ready merely because the builder finished.

# Route markers

- `review_route:build_candidate_to_review_pr`
- `review_route:repair_returns_to_same_candidate`

# Candidate operating contract markers

- `candidate_contract:host_shell_detection`
- `candidate_contract:focused_local_proof`
- `candidate_contract:one_writer_worktree`
- `candidate_contract:publish_for_remote_evidence`

# Environment and ownership

Identify the actual shell, host/target platform, repository remote, root, worktree, branch, HEAD and available toolchain. Shell grammar is not inferred from OS: PowerShell can run on Linux; Bash/WSL can run on a Windows host. Read `docs/agent-context/validation.md` for native exit-status handling and proof ownership.

One writer owns the candidate worktree at a time. A delegated writer receives the claim, input identity, write boundary, non-goals, proof and handback; the root does not edit concurrently. Reviewers read committed objects or create their own detached inspection worktree. Preserve pre-existing changes and never move another writer's HEAD.

Before allocating a worktree, starting a build, or freezing an executable, apply the [bounded storage closeout](../../../docs/PR_AUTOMATION.md#bounded-local-storage-and-proof-retention) procedure. The existing root/task owns each exact resource, its peak-growth budget, required witnesses and planned disposition before generation. Include frozen copies and restoration staging in the budget; a Cargo build budget alone does not cover them. Release or recoverably archive no-longer-needed outputs from completed candidates before admitting the next batch. This is root-owned admission using existing tooling, not a new registry or an automatic cleanup claim.

Bind background commands to retained task/session handles, candidate identities and logs. A process-name filter, quiet interval or stale report does not establish failure or orphaned work. Read the native exit and terminal report, not stderr noise or a success-looking line alone. Serialize Cargo operations that share a worktree, target lock or memory bottleneck. Do not kill unrelated processes. The parent goal may advance another ready claim on a separate worker/worktree while this candidate waits.

# Procedure

1. Bind the issue, acceptance/rollback boundary, existing candidate, exact base and semantic owner. Re-read current source, all-state/recent PRs and substantive issue decisions for an equivalent implementation before editing.
2. Establish the inherited baseline before the first mutation:
   - run `cargo xtask worktree doctor` and read its report;
   - verify `git merge-base origin/main HEAD` equals the recorded base before using `check-fast` for base attribution. If it differs, reconcile the accepted basis or run base-aware gates directly; a mismatch is not a demand to chase unrelated main movement;
   - independently require `git diff --name-only origin/main...HEAD` to succeed. A zero-path result is expected only when HEAD is the recorded base, not when the selector failed;
   - run `cargo xtask check-fast` on the new candidate's base. This policy floor is not compile or full-suite evidence;
   - run the narrowest `cargo check` covering the owner on the exact base, normally `cargo check -p <package> --all-targets`. Widen for shared manifests or cross-package changes. Record the omitted compile dimension for non-Rust-only work;
   - when resuming an already-mutated candidate, reproduce an apparently inherited failure in an isolated exact-base worktree before blaming the base. Use full `precommit` there only for a baseline-repair claim or an authoritative comparison;
   - if local execution is unavailable, retain that missing baseline dimension and use the available hosted route. Do not fabricate a local pass or turn tool absence into a blanket prohibition on PR publication.
3. Read production consumers, tests, fixtures and the strongest wrong/boundary case. Route unrelated inherited failures to their own owner rather than absorbing them into this patch.
4. Establish a discriminating control before repairing production:
   - add the smallest test/fixture/artifact assertion that rejects the missing behavior;
   - observe its intended failure against the pre-repair state;
   - for an existing fix, bind the observation to its parent or a reversible wrong-implementation/removal experiment;
   - report an unexecuted red dimension as `NOT_ESTABLISHED`, not an invented red/green pair. A compile error before the behavior runs is not the intended behavioral discriminator.
5. Implement the smallest coherent repair in the owning layer. Do not create a parallel validator, route or authority for convenience.
6. Rerun the focused control after coherent edits. Classify source, test/oracle, instrument and infrastructure failures before choosing a repair. Do not retry a deterministic failure without changing or investigating its cause.
7. Harden the proof: positive and negative cases, nonempty intended subjects, setup assertions, currentness/identity, explicit limitations, and rendered/public behavior where source-text coincidence could otherwise pass.
8. Simplify the candidate: remove scaffolding and dead branches, collapse duplicate decisions, and preserve one acceptance/rollback boundary. Do not mix unrelated dependency, documentation and product changes because they share a working tree.
9. Challenge authority/provenance, failure and rollback paths, transaction boundaries, concurrency, oracle grip, runtime/schema/docs/output parity, platforms, packaging and user-facing claims. After focused tests, retain the context-rich session for cheap design and test-oracle challenges before another expensive build. Try the strongest counterexample, a false assumption, a plausible bad implementation the tests accept, and relevant recovery, compatibility or resource failures. Revise accepted defects before execution; reuse valid test evidence when source, inputs and contract are unchanged, and rerun affected proof when behavior changes. Publish concise conclusions in the existing PR body or owned comment: proposition challenged, evidence or counterexample, disposition, remaining uncertainty, exact candidate and next step. Keep private scratch reasoning out of GitHub. Cached context is a resource advantage, not independent review: a fresh exact-head reviewer through `review-pr` challenges blind spots; existing provider reviews remain separate evidence. Do not repeat equivalent reviews or spend calls solely to fill a role.
10. Repair accepted findings in the same candidate. Commit coherent changes without another routine permission pause so verification can bind to a real Git object.
    Before finalizing the candidate for publication or Draft -> Ready hosted
    qualification, record its changelog disposition in a local draft of the new
    PR body or update the existing PR body:
    - Name the fragment under `changelog.d/` and compare it with the canonical
      example in [changelog.d/README.md](../../../changelog.d/README.md): first-line
      section marker, `- ` bullet and trailing issue/PR reference.
    - If no fragment is needed, explain the exemption under
      [CHANGELOG_POLICY.md](../../../docs/CHANGELOG_POLICY.md), checking behavior,
      workflow and public-doc impact per [CONTRIBUTING.md](../../../CONTRIBUTING.md#pr-shape).
      A deferred entry instead needs the release-inventory location and reason
      required by [PR_AUTOMATION.md](../../../docs/PR_AUTOMATION.md#changelog-inputs).
11. Run `check-fast` on the committed candidate and compare its selector report and ran/skipped categories with the independently resolved path set. Reconcile a changed basis or run base-aware gates directly. Unexpected zero, omitted categories or selector failure is `INSTRUMENT_FAILURE`, not pass.
12. Run focused tests and relevant changed-surface checks before `precommit`. For targeted-rerun owner/call admission changes, run the affected retained CLI rerun tests and the `native_path_roots` integration suite, including supported imported and qualified free calls plus strict binding-refusal controls. `check-fast` policy/Clippy selection and library-only tests do not establish this public-path compatibility. Preserve legitimate fixture call identity instead of rewriting calls to evade the new gate. For process-launch API or test-harness command-builder changes, explicitly run `cargo xtask check-process-policy` before the aggregate and reconcile call sites with their existing allowance; Rust tests and Clippy do not validate that allowance. Keep proof proportional: full local runs are for named failures or explicit qualification, not automatic duplication of the entire hosted matrix before publishing.
13. Hand the committed candidate to `review-pr`. Missing hosted evidence normally yields `REVIEW_INCOMPLETE`; enter `finish-pr` to publish and obtain that evidence, then return to exact published-head review before merge.
14. On `REPAIR_REQUIRED`, repair the same candidate and refresh affected proof/review dimensions. Before resolving an integration conflict, check whether upstream already delivered the claim; preserve only a genuine unique residual.

# Currentness and delivery

A behind-only branch does not require restacking. A material content conflict, prerequisite change, combined-tree failure or governing exact-base rule does. Track implementation, stimulus, oracle, public claim, generated relationships, conflict resolution, integration basis and head identity separately; do not invalidate unaffected evidence just because main moved.

Local commits and test runs are useful unpublished candidate evidence, not landed repository delivery. A reported test count or subagent conclusion must resolve to the same subject and execution before it supports a PR claim.

# Decisions and cleanup

Choose reversible in-scope implementations from evidence. Routine commit/push/PR/review repair/protected merge is already inside an authorized delivery goal. Ask only at an actual scope, destructive-action, exposure, settings, release-authorization or non-derivable product boundary. Do not create rival candidates, reservation files, overlap maps or sibling monitoring.

After a durable merge/closure handoff, apply the [bounded storage closeout](../../../docs/PR_AUTOMATION.md#bounded-local-storage-and-proof-retention) procedure to lane-created worktrees, branches and temporary residue. Return each exact resource as `released`, `retained with reason`, or `awaiting owner verification` before removal. Releasing ownership alone is not deletion authority. Preserve retained and unknown resources, required proof and unrelated work.

# Valid exits

- `CANDIDATE_READY_FOR_REVIEW`
- `CANDIDATE_REPAIRED`
- `PROOF_NEEDS_REPAIR`
- `PLAN_OR_ISSUE_NEEDS_REPAIR`
- `REPAIR_REQUIRED`
- `REVIEW_INCOMPLETE`
- `INSTRUMENT_FAILURE`
- `INFRASTRUCTURE_FAILURE`
- `EXTERNAL_BLOCKER`
- `NOT_ESTABLISHED`

A lane exit returns its exact object, evidence, residual and next transition to the parent loop; it does not complete the parent goal.
