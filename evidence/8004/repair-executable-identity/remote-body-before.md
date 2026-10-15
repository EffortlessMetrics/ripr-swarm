## Summary

Refs #7415. Direct inherent-method attribution requires bounded receiver evidence. The cumulative repair refuses name-only, comment, expired-binding and item-shadow evidence while preserving qualified variants and live outer receivers. This wave repairs the three independent review findings: executable same-name local delegation survives declaration filtering; inherent helper entry/last-hop identity is checked; assertion-macro scope refusal agrees across consumers.

## Swarm / Source Boundary

- [x] Repair on the existing same-repo Draft branch; no replacement PR.
- [x] Source, regressions, fixtures, spec/traceability/changelog and observed corpus evidence form one attribution seam.

## Source-of-truth Links

- Governing issue: #7415 remains open.
- Specs: RIPR-SPEC-0001, RIPR-SPEC-0005, RIPR-SPEC-0108; observation honesty RIPR-SPEC-0219.
- Independent source findings: https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#pullrequestreview-5529532589
- Current canonical technical handoff: https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#issuecomment-6186356861
- Existing analyzer ownership; no new architecture or support-tier expansion.

## Candidate Relationships

Existing branch `rook/7415-private-target-attribution`. Source `53e22999d19d07a28a355b740d62c01c25aea541`, tree `07100736ea448043831337f64d45c7a26b6d1b79`, preserves parent `43f00b54eeb239c240bb71553c30a7bde3a4e080` and its integrated suites. No behind-only restack or history rewrite. Frozen runtime composition `72e25151dd03a669575c57175321072e69b24d42`, tree `c80d13b636ac00b1fc695c5f41ff42e058183556`, combines inspected main `7dee26dd330fd648ed4f8ba46225cc78276fb147` and source53. Later inspected main `37a31f949` composes textually without conflict; it has no runtime receipt here.

## Scope Classification

- [x] One production behavior / architectural seam: bounded inherent owner attribution.
- Production authority: shared direct-owner receiver/module checks; bounded same-file executable helper walk; declaration artifacts excluded without blanket self-name suppression.
- Evidence: three fixture inputs, six paired native controls, full/compact matrix, production diff integration, three complete report projections and honesty assertions.
- Non-goals: general type/Cargo-target resolution, arbitrary factory/parameter/field/cross-file flow and complete macro expansion. Conservative unsupported attribution can still yield bad advice; five existing observations remain false_actionable.

## Support-tier / Policy Impact

No support-tier, dependency/lockfile, public schema, workflow, process/network allowance, oracle floor or policy exception change. Canonical Fixed changelog and spec/traceability mappings included. Performance #8005 remains unbenchmarked and separate.

## Proof and Claim Boundary

The canonical handoff and immutable packet below bind commands, counts, failures and identities. Local tests use default features; historical Rust-only hosted jobs do not qualify this head. The previous golden explanation is corrected: a separate documented locked anyhow1.0.100 fetch makes prepared base/final complete JSON equal and exposed. Full453 goldens now pass, zero drift/errors; no existing snapshot was re-blessed. All five named corpus rows retain runtime truth/ideal/acceptable labels and false_actionable outcomes.

Draft / REVIEW_INCOMPLETE. Rook assigns fresh independent published-head review. Required hosted default-feature proof, full local workspace-test and Windows qualification are not established. No Ready/merge clearance or external-thread resolution.

## Spec-Test-Code Traceability

- Shared classify receiver authority and detailed/compact repository relation consumers.
- Native `private_inherent_receiver`, full/compact receiver matrices and declaration/call graph regression; .ripr/traceability.toml maps the affected spec claims.
- Three new fixtures bind macro discard, foreign helper and genuine local wrapper; six controls challenge entries, last hops, owner-type shadows and standard/sibling macro positives.
- Existing qualified-variant/live-receiver and both appended suites preserved; corpus truth remains independent of observed advice.

## Rollback

Normal revert of bounded source/tests/fixtures/spec together restores the demonstrated attribution failures. Corpus observations must continue recording actual behavior rather than changing runtime truth to fit the analyzer.

## Static Language / Engineering / CI Economics

- [x] Conservative static-exposure vocabulary; no claim of runtime correctness or test adequacy.
- [x] No panic shortcut, new dependency, process/network surface or public JSON/schema change.
- [x] Original candidate and unique evidence retained; no destructive cleanup.
- [x] Relevant local semantic, corpus, golden, integration, selector and policy checks.
- [ ] Fresh independent published-head review and required hosted default-feature qualification.

No workflow/protection change or manual release lane. Keep Draft. The handoff below is the current source/proof record; historical packets and failed attempts remain inspectable.


## Current Repair Receipt

**PR_IN_FLIGHT / REVIEW_INCOMPLETE — repaired candidate published for independent review.**

Source `53e22999d19d07a28a355b740d62c01c25aea541`, tree `07100736ea448043831337f64d45c7a26b6d1b79`, preserves parent `43f00b54eeb239c240bb71553c30a7bde3a4e080` on the existing `rook/7415-private-target-attribution` branch. The replacement environment is usable; no startup or unpublished-work blocker remains.

The three findings in [independent review](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#pullrequestreview-5529532589) have source repairs and paired controls: declaration artifacts are filtered while executable same-name local-receiver delegation retains helper/high repository credit; foreign/unknown inherent helper entry and last-hop receivers cannot acquire owner credit by method-name coincidence; diff assertion operands now use the same module-aware macro-binding refusal as repository consumers. Qualified variants and live outer receivers remain supported. Receiver identity remains bounded nominal syntax; arbitrary factory/parameter/field/cross-file flow, general type/Cargo-target identity and macro expansion remain outside this claim.

Local default-feature evidence: exact committed head passes383 test-grip tests,175 related-tests tests,43 CLI rerun units, all453 golden fixtures with zero drift/errors, all four verdict corpora (36 Perl/69 Python/485 Rust/169 TypeScript), all15 check-fast gates without skips including strict workspace/all-target Clippy, and complete58-path precommit/selector checks without limitations/skips. Same production bytes before commit also pass8 receiver and5 native-path integration tests,16 CLI rerun integrations, unit-struct static/runtime control, honesty and nonempty dogfood controls. Six additional committed native controls compile/run originals and one-arm mutants: positives kill the mutant, negatives survive; all originals pass. Full/compact unit matrix and complete raw CLI outputs remain retained. These are selected local runs, not full workspace or hosted qualification.

**Correction to the previous golden explanation:** missing documented locked anyhow1.0.100 fixture preparation caused the prior local drift. Ordinary registry `cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml` fixes it; prepared baseline/repair complete JSON are equal and exposed, matching committed findings/summary. No existing golden was re-blessed. Only three new fixtures received initialized snapshots. The five named existing corpus observations remain `false_actionable`, with runtime truth/ideals/acceptable labels unchanged. Rust corpus retains26/485 false verdicts, contradictions0/621, oracle44/44, overclaims0/44.

Executed composition `72e25151dd03a669575c57175321072e69b24d42` (tree `c80d13b636ac00b1fc695c5f41ff42e058183556`) combines inspected main `7dee26dd330fd648ed4f8ba46225cc78276fb147` and candidate53. It passes383 test-grip tests,175 related-tests units,23 assertion-helper integrations,8 receiver integrations and5 native-path integrations, zero failures/ignored in those selections. The three affected fixture comparisons use a retained source53 driver that builds the actual clean composition CLI; that runner/normalizer code is unchanged. Later main `37a31f949` was inspected (MCP budget plus unresolved deep-transitive witness naming) and composes without conflict as tree `665505641bd21f018359268ed614a34b205bf45b`; no execution claim for that later tree. No source restack. Immutable evidence link and manifest are supplied below. Earlier failures (intended red regression, temporary E0515 compile failure, staged whitespace failure, invalid stale3865 CLI instrument) remain retained and classified. Original43 source and all historical packets remain preserved.

Keep Draft. Fresh independent review of the published repaired head belongs to Rook's next assignment; required hosted default-feature qualification remains outstanding. Historical lang-rust-only job116287116012 and old15980 counts are not this qualification (the latter included skips/ignored subjects). No Windows/full local workspace qualification or performance clearance; #8005 remains separate and #7415 remains open. No review threads resolved, bot contact, Ready, merge or product publication/release actions. Rollback is a normal revert of the bounded source/tests/fixtures/docs together; observation truth must remain honest.

Local source/composition worktrees, target/cache and frozen executables are retained for this open Draft's review/reproduction. Immutable evidence excludes executables and volatile coordination files. Next transition: Rook assigns independent exact-head review, then reconciles required hosted proof within the Draft workflow.


[Immutable repair evidence](https://github.com/EffortlessMetrics/ripr-swarm/tree/776209c1eab8512696f50fedcaf555028b6394b2/evidence/8004/repair53): evidence-only commit `776209c1eab8512696f50fedcaf555028b6394b2`,279 files /3,305,581 bytes; all278 manifest entries verified after remote fetch. Manifest SHA256 `1c7cedcbdee5760cd0c4e9de48d9bcd828506caa32593db6f76b88912b29262d`. Its parent preserves the executed composition object and both source/main parents. No rival PR. Raw reports, selected-suite logs, failures, baseline comparisons, input digests, native controls, policy/selector reports, author self-review, limits and original handoff/body are retained.

