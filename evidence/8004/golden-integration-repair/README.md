# #8004 bounded golden integration repair

Candidate `5e34bad2a9d066387d72777f37d1134987c27698`, tree `8a27c0f5dfa06586715b68b448a04625aac5f1f9`, parent `36dbb87430ef858aa143b5fc3205b376b0caee11`. Existing canonical branch `rook/7415-private-target-attribution` and Rook ownership retained. This writer owns only the delegated CI repair; fresh independent exact-new-head review remains required.

## Failure and provenance

Required run38861065156/job116843321438 actually executed merge4232b0459b58eb3283c9f9d1ee42faf690679446 (d08/36db, tree024d). It passed its earlier required test/check/Clippy/policy steps, then failed `cargo xtask goldens check`. The retained log reports15 human/human-full drifts across11 inherent_receiver fixtures, rather than the handoff's16. Later fixtures, verdict-corpus and PR-evidence steps were skipped, not passes. The old failure is retained; this repair is not a hosted rerun or all-required-check pass.

Native Git blame and the complete relevant patch trace all60 changed bytes to landed fdcb75cf3f9acc928967d8ab4764d0caf8dbb31b (#8096/#7756): labelled human fields and list-item values drop one terminal period at the emit boundary. `render_analysis_outcome_disclosure` changes Analysis outcome/Counts punctuation; human triage changes Safe next action; `trim_field_period` changes next-step/shared-guidance/weakness values. Ellipses are preserved. Producer strings/JSON remain independent from human presentation.

## Pre-edit gate, including the unexpected local semantic refusal

No expectation was edited until a full unchanged-source run and strict audit established presentation-only drift. The first local full-golden attempt also reported `owner_return_pin_fallible_literal` exposed→weakly_exposed JSON/human drift. The strict audit rejected that semantic difference and stopped. Its actual reports, traceback and initial golden failure are retained.

Fixture setup investigation found exact `anyhow = 1.0.100` missing from the local cache (only1.0.104 present); `cargo metadata --locked --offline` exited101 trying to download the required version. `cargo fetch --locked` populated precisely1.0.100; its crate checksum a23eb6b1614318a8071c9b2521f36b424b2c83db5eb3a0fead4a6c0809af6e61 equals the unchanged fixture lock. Offline metadata then exited0. No fixture source, dependency declaration, lock, truth label or expected verdict was changed. The full unchanged-source rerun removed that semantic drift and left exactly the15 hosted human drifts. Binary build identities are retained separately; no cross-attempt binary-digest equality is claimed.

The completed pre-edit audit compares every expected/actual golden JSON:465 complete stored objects and normalized complete bytes equal. The unchanged repository normalizer projects paths and omits canonical_next_action. Eleven additional unprojected affected-fixture CLI reports retain that member: every original JSON member equals the parent golden, and the canonical stop case matches the original human state. Raw parent canonical-member equality is not claimed because that member was absent from stored goldens. Every human difference is exactly removal of one terminal ASCII period; no changed token, number, class, evidence, recommendation content, row, path, identity or line count. All15 file differences and every old/new line are retained. Four deliberate in-memory wrong-result challenges (classification token, probe denominator, empty human output, JSON classification) are rejected. `formatting_only` alone was not accepted as semantic proof.

## Exact repair boundary

A guarded line-only edit removes60 terminal periods from15 human files. Eleven existing per-fixture CHANGELOG.md files gain explicit integration justification and verification instructions. `goldens bless` is not invoked. Complete Git inventories establish that the other12,154 tracked blobs are identical, including all3,264 tracked JSON files, production Rust, manifests/locks, native controls, honesty/corpus data and generated claims. The honesty corpus subtree stays91c34f000dae8d8a9f2c3b4f6b2985cb13b7d115. No accuracy label or supported behavior changes. Reverting this text commit is the complete rollback.

A new root release fragment is exempt as mechanical expectation synchronization under docs/CHANGELOG_POLICY.md; the inherited #7756 fragment already describes the public behavior. Per-fixture reasons identify #8096/#7756 and this #8004 integration repair.

## Validation and limits

The authoritative command/exit/report list is in receipt.json and raw/post.json. Baseline check-fast passes15 gates on the independently verified173-path selector. `worktree doctor` exits1 because main advanced seven commits; this is retained behind-only hygiene, not a source failure or instruction to restack. Current tracking main895e4a47db94a618504875debbd1b76e4ed951f7 has merge-base d08 with this candidate. The source proof stays bound to its actual candidate/basis.

Exact committed-head validation includes the full golden suite, fixture execution, honesty checks, check-fast, period-rendering units, receiver integrations, negative human-semantic-oracle units and typed precommit. Selected/executed subjects, external opt-in omissions and failure history are recorded, not inferred from aggregate green. No broad benchmark or full required hosted matrix is duplicated. Native truth preservation is by complete unchanged bytes/prior proof; no fresh full native mutation matrix is claimed.

Latest-main composition is a separate subject:895e+candidate recomputes cleanly as treea1bd993f2d49c2ac4f95b125bd52410be0fefc67, unexecuted. Later main includes #8161 classifier/render changes; no latest-main/product-runtime qualification is claimed from a clean merge or these source-head checks. Delivery owner must inspect the actual post-push merge subject and fresh review/check applicability before separately authorized qualification. No endless behind-only restack is performed here.

Original residual latency, per-query work/unique-key memory, censored samples and unrun warms remain unchanged limits. This text repair does not remeasure or clear them.

No Ready/Draft change, PR merge, thread resolution, issue8005 access, bot contact, manual CI rerun, release/deploy, security/credential change or unrelated cleanup. Worktree `/workspace/review36db`, shared task-owned target `/workspace/review36db-target` and raw `/workspace/golden8004-evidence` are retained for the parent/fresh reviewer; inherited root `/workspace/ripr-swarm` is untouched.

## Source publication and actual-subject readback

Ordinary non-force push advances the existing canonical branch36db→5e34bad2a9d066387d72777f37d1134987c27698; PR readback confirms open/Ready unchanged. All eight exact-head validation commands exit0. Golden/fixture suites each pass465 fixtures/1,157 compared surfaces; all465 JSON goldens match, and11 additional raw JSON controls pass. Fourteen focused tests pass (2 period-rendering,9 receiver,3 negative human-semantic-oracle), no ignored subjects. check-fast passes15 gates with no skips and the exact173-path selector; typed precommit passes on the same173 paths, zero skips/limitations. Configured judged-panel departures remain in its raw log; no accuracy-cost clearance follows.

GitHub now advertises merge740b6da9b2cc9fd402b9f588fe1c1f972d64745a, parentsfabf21c8f8e5a06e61cb3e7c77c96f713dec8a13 /5e34bad2a9d066387d72777f37d1134987c27698, tree778347a67ee2953192d7fd35ffcfde72175c7877. This differs from tested source tree8a27 and from the earlier prospective895e treea1bd. PR base metadata still readsd08, so it is not substituted for the actual merge object's parent. This new combined subject was not executed here; parent owns fresh exact-head/integration review and any separately authorized qualification. No claim from unrelated #8229 is used.

Disposition: CANDIDATE_REPAIRED / FRESH_INDEPENDENT_REVIEW_REQUIRED. The prior review of36db is historical, not clearance of this expected-output change/new integration subject. No Ready transition, merge, thread resolution or manual CI rerun. Source worktree/target/raw witnesses remain for the parent and reviewer.
