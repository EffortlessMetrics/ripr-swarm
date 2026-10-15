## Summary

Refs #7415. Bounded inherent-owner attribution requires nominal receiver identity plus complete owner/module refusal, excludes dormant item/closure/async bodies, and binds same-file helper entries to actual declarations. Standard assertion operands now share the executable-node exclusion with the outer walk, so a discarded unpolled future supplies neither direct nor free-helper owner credit. Genuine outer calls remain admitted.

## Swarm / Source Boundary

Existing same-repo Draft branch `rook/7415-private-target-attribution`. Normal history-preserving merge and push; no replacement product PR or restack. The artifact branch contains evidence only.

## Source-of-truth Links

- Governing #7415 remains open; specs0001/0005/0108 and observation honesty0219.
- [Current technical handoff](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#issuecomment-6194778359).
- [Immutable raw packet](https://github.com/EffortlessMetrics/ripr-swarm/tree/2377b7f5b19905e3cdcfb8919faeed2e0f525ca8/evidence/8004/async-operand-integration).
- [Independent source8b finding](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#pullrequestreview-5532955967), now reproduced with actual CLI/native controls.
- [Preserved source8b handoff and proof](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#issuecomment-6191731802).

## Candidate Relationships

Head `a2b489b9d7d5b78daebf9fc1b3e4201dae3c9524`, tree `6bd2c8c772cfbb11eb9c8ae709fff98d465e7f85`. Parent `013ea85816511a13f13d9728526d3337aecd17dc` merges source8b and executed basis `a142074061128a85e76c1ac494b9939f3a58428b`, preserving the earlier base678/source history.

The actual corpus conflict was resolved by complete keyed three-way comparison:294 base/309 candidate/297 main ->312 cases, no divergent same-ID edits. All input-side rows/metadata preserved; four new cases give316.

Later main `8a55a5be744368ff3592709957451c6ba04d8a4a` was inspected. Its clean synthetic tree `8feb57a27f0211ae136217cf8620ba542595c1d9` matches GitHub merge `ef9978d9445b89486bcdb2dbe6360e5a7204371a` (parents8a55/a2). **That later tree was not executed.** No behind-only branch update.

## Scope Classification / Policy Impact

One bounded execution/owner-identity contract. The34-case matrix preserves previous22 receiver/helper/scope controls and adds12 assertion-operand controls. Native execution and static admission remain separate; invoked nested functions remain conservatively unsupported. No general type/Cargo-target, factory/parameter/field/cross-file flow or macro expansion.

No authored dependency, workflow, public-schema, credentials/security/process-policy or support-tier change. Repeated parsing and per-call OwnerPinSyntax cache loss remain substantive/unbenchmarked under #8005.

## Proof and Claim Boundary

Exact-a2 selected unit/integration checks:11 commands pass, including full/compact matrix, related tests, owner pins, caller identity/scope, receiver/helper/native-path/rerun and current-main suppression controls. Clean a2 CLI passes102 matrix reports plus three two-subject same-line reports. Native matrix68 executions and four extra filtered runs retain originals/mutants independently.

All15 fast gates pass without skips, including full workspace/all-targets Clippy; independent141-path selector equals the complete policy inventory. All463 selected/actual/compared goldens pass, zero drift/errors. Two new fixtures initialized; no old golden re-bless. Rust/Python/TypeScript corpora, honesty and nonempty dogfood pass. Inaccurate verdicts remain visible (Rust21/485; TypeScript7/169); five prior false_actionable observation blobs and case/truth bytes are unchanged.

**Incomplete:** final precommit returned1 after capacity-interrupted selected xtask Clippy; its raw failure remains retained. Perl replay was not admitted (preflight73) to preserve8GiB reserve. Full workspace Clippy does not substitute for that incomplete precommit configuration. Latest-main execution, fresh independent review, required hosted checks, Windows and full-workspace runtime qualification remain unestablished.

**PR_IN_FLIGHT / REVIEW_INCOMPLETE — keep Draft.** Rook assigns exact-head independent review and resumes missing qualification with sufficient capacity. No Ready/merge clearance.

## Spec-Test-Code Traceability / Changelog

`executable_call_resolves` and `assertion_arguments_resolve` share `node_executes_in`; existing complete owner refusal and free-helper declaration custody remain intact. Full/compact and production diff matrix tests consume the34 complete stimuli. New `inherent_receiver_async_operand` and `inherent_receiver_async_operand_helper` fixtures, actual raw corpus reports, spec0005 and `.ripr/traceability.toml` map the boundary.

Updated `changelog.d/7415-inherent-receiver-admission.md`; canonical Fixed section, bullet form and issue/PR references checked against the fragment README. No deferred changelog entry.

## Rollback

Revert bounded authored code/tests/fixtures/docs/corpus additions coherently; retain independent runtime truth. Integration parents and historical proof remain explicit.

## Static Language / Engineering / CI Economics

- [x] Conservative static-exposure vocabulary and independent native controls.
- [x] Exact-head selected checks and frozen-main integration receipts.
- [x] Preserved source8b proof, bad-advice observations, failed attempts and resource receipts.
- [ ] Capacity-complete precommit and Perl replay.
- [ ] Executed applicable latest-main composition, independent review and required hosted qualification.

Evidence-only commit `2377b7f5b19905e3cdcfb8919faeed2e0f525ca8` retains2368 files/2367 manifest entries; manifest SHA256 `a6c886a9983ce14d600654e9d699c41280f84613ddde83e1c450d1c4471925ab`. Source/evidence refs and published README verified. Resources remain retained for open-Draft review; no destructive cleanup, Ready, merge, release/deployment, thread resolution or bot contact.
