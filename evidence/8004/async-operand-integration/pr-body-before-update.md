## Summary

Refs #7415. Bounded inherent-owner attribution now uses the complete owner refusal for repository direct calls, excludes dormant nested bodies from executable evidence, and resolves same-file helper entries by actual module/function identity. Adjacent declaration type-name shadows and same-line caller scope memo collisions are also refused. Qualified/live receivers, valid same-name inherent wrappers and assertion-macro repairs remain supported.

## Swarm / Source Boundary

Repair on the existing same-repo Draft branch `rook/7415-private-target-attribution`; no replacement PR or history rewrite. Source/tests/fixtures/spec/traceability/changelog and honest observed corpus reports form one owner-attribution seam.

## Source-of-truth Links

- Governing issue #7415 remains open; specs0001/0005/0108 and observation honesty0219.
- [Current technical handoff](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#issuecomment-6191731802).
- [Independent source53 findings](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#pullrequestreview-5531551872); all three reproduced on exact source53 and parent43 before repair.
- [Immutable raw evidence](https://github.com/EffortlessMetrics/ripr-swarm/tree/62c8fe9d4f256f11e31f21f1eb151d8fbfac5a76/evidence/8004/repair-executable-identity).
- [Historical source53 handoff](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#issuecomment-6186356861); prior work and failures preserved.

## Candidate Relationships

Current source `8bbc8dd4dead78cdccdb10861fe4848103aac6dd`, tree `cda6f3f58de043bd5615caaa42984ff06024d7bb`, preserves source53 through commits3a0b9379 and8bbc8dd4. Merge base `67866bc95a72462840c3eedac0f213ed658474b4`.

Executed composition `ecb5173f6cef7c2eaedfde84a17330c27a7cafbb`, tree `1f2daf30baf9c12b4cce760d167945de43c2dec5`, combines main9f7d06bf and source8b. Keyed three-way corpus merge preserves both sides,311 cases, zero semantic conflicts. Later main87528ce8 inspected; only appended corpus textual conflict, no runtime qualification claimed for that later tree. No behind-only restack.

## Scope Classification

One bounded executable-owner-identity contract: shared direct refusal, executable syntax and same-file helper declaration custody.22-case hypothesis matrix plus an extra two-subject same-line cache control; full/compact/diff CLI and independent native original/mutant controls.

No general type/Cargo-target analysis, factory/parameter/field/cross-file value flow, invoked nested-function interpretation or full macro expansion. Valid helper credit is repository-specific; diff-helper support is not claimed. Existing five false_actionable observations and runtime truth/ideal/acceptable labels remain unchanged.

## Support-tier / Policy Impact

No dependency, public schema, workflow, credentials/security/process-policy or support-tier change. Repeated parsing and per-call OwnerPinSyntax cache loss remain substantive and unbenchmarked under #8005; correctness tests do not refute them.

## Proof and Claim Boundary

Exact source8b passes385 grip/175 related/33 identity/8 scope/43 rerun units;9 receiver/5 native-path/21 assertion-helper/17 rerun integrations.457 goldens pass with zero drift/errors; four corpora, honesty and nonempty dogfood pass. All15 check-fast gates and precommit pass; independent115-path selector match, no skips/recorded limitations. Documented locked anyhow1.0.100 fixture preparation done. Four new fixtures initialized; no old golden re-bless.

66 raw boundary CLI reports verify intended subjects, four owner seams per full case, full/compact metric parity and all diff oracle rows. Five complete new corpus reports match final-head CLI after excluding only volatile artifact metadata.48 current-wave native executions are retained separately from32 independently replayed historical reviewer executions.

Executed composition passes387 grip/175 related/33 identity/8 scope/43 rerun units and9 receiver/5 native/23 assertion-helper/17 rerun integrations; its66 CLI decision projections equal source8b and three extra same-line producer reports pass. These are selected local checks, not full-workspace or hosted qualification.

Evidence-only commit `62c8fe9d4f256f11e31f21f1eb151d8fbfac5a76`:2951 files /16,640,891 bytes; all2950 manifest entries verified after remote fetch. Manifest SHA256 `d0d56afe8adbebb39a9a3db149ecb1e5b33f5c819e9db9c2b6f1dc67dd0bdf12`. Includes raw failures, self-review, resource/recovery receipts and exact identities; excludes executables, caches and scratch drivers.

**PR_IN_FLIGHT / REVIEW_INCOMPLETE. Keep Draft.** Rook assigns fresh independent review. Required hosted default-feature proof, Windows and full local workspace qualification remain unestablished; no Ready/merge clearance. Parent's12:08 disconnect checkpoint was recovered without duplicating composition; source/evidence remote refs verified.

## Spec-Test-Code Traceability

Shared classify owner/executable/refusal predicates and repository detailed/compact consumers; syntax namespace custody; production diff integration; four new fixture inputs and22-case native matrix; .ripr/traceability.toml maps spec claims. Existing qualified/live receiver, same-name wrapper and assertion suites remain exercised.

## Rollback

Normal revert of bounded source/tests/fixtures/docs together restores the demonstrated failures. Keep corpus runtime truth independent of analyzer advice.

## Static Language / Engineering / CI Economics

- [x] Conservative static-exposure vocabulary; no runtime-correctness or test-adequacy claim.
- [x] Existing source and raw failed attempts retained.
- [x] Affected local native/integration/selector/policy proof and executed later-main composition.
- [ ] Fresh independent published-head review and required hosted default-feature qualification.

No Ready, merge, release/deployment, thread resolution or bot contact. Owned worktrees/target/frozen CLIs are retained for open-Draft review.