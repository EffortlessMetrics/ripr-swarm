# Independent candidate review

## Latest reviewed merge successor: `a97ec757ba2bc84292729ee8675d0f5459d3808a`

reviewed_head_sha: `a97ec757ba2bc84292729ee8675d0f5459d3808a` (tree `1742c3f7ceb9c5556571dbc28a82f90af9664283`; parent `704c4f611c3586073e1efb322ae6fcd60c013f90`)
integration_basis: frozen main `a84449ad6cbbe026f838cab656ad82993ccdc5e3`, integrated as the second parent of 704; the composed candidate retains the repair on top.
claim_boundary: local source and exact-head evidence for PR #7817's Some(false) implicit-observer refusal / independently confirmed token behavior and merged sibling-exclusion admission seam. Hosted PR qualification is outside this pre-publication review.
changed_surfaces_and_semantic_owners_inspected: the only delta from 704 is deletion of the duplicate neutral `assertion_has_known_implicit_observer: ... None` initializer in `crates/ripr/src/analysis/classify/reveal.rs`'s refusal regression. The intended `Some(false)` callback remains, as does `sibling_exclusion_admitted: ... true`. This removes 704's E0062 duplicate-field compile failure without changing production logic, the test's refusal stimulus, or main's sibling-exclusion branch. Previously reviewed admission/effect/token gates and run-scoped expansion memo are byte-identical to 704/008.
blocking_findings: none found on a97 source. The compile blocker previously reported on 704 is repaired here.
non_blocking_suggestions: none.
refuted_or_stale_findings: 704's duplicate-field compile finding does not apply to a97: the exact diff deletes only the second `None` initializer, preserving `Some(false)` and sibling-exclusion `true`. The earlier 008 + 1cdc composition is stale as current-main integration evidence; the current frozen-main merge lineage is 704 -> a97.
proof_and_ci_observed: Author-retained v5 receipts all bind a97/tree1742 and exit 0: public 14/14 (`sibling_effect_oracle`), reveal 114/114, expansion 2/2, transitive reach 66/66, native roots 5/5, CLI rerun 17/17, goldens, evidence-promotion honesty, dogfood, check-fast, and precommit. The check-fast log reports 15 gates and 55 changed files; the retained 55-path selector list exactly matches the independently computed `git diff --name-only a844...a97` list. The precommit receipt records explicit exit 0 at 09:33:54 UTC after a reported process disconnect; it was not rerun. These are author executions, not reviewer executions. Earlier 704's reveal attempt failed before tests on the duplicate field, a separate historical identity. The frozen-main integration basis is a844, merged through 704 and repaired in a97; the earlier 008 + 1cdc composition remains separately identified and is not used to qualify this candidate. Fixture-level native controls in `token-native-controls/` establish runtime sensitivity of fixture assertions, not an analyzer mutation.
proof_or_review_missing: hosted checks, artifacts, comments/review, and published remote PR head are not observed. No analyzer wrong-behavior mutation was run in this review; the existing control experiment is fixture-level only.
affected_currentness_dimensions: candidate head, source, test initializer/stimulus, frozen-main merge basis, and exact-head local validation refreshed to a97; hosted/platform dimensions remain unobserved.
residual_assumptions_and_non_claims: this review establishes the committed local tree and its recorded receipts only. It does not establish latest live-main ancestry beyond the explicitly frozen a844 prerequisite, hosted CI, external review, or readiness to merge. #7880's external corpus setup issue remains separate.
disposition: `REVIEW_INCOMPLETE` pending hosted PR qualification. No source finding remains on a97. #7880's external corpus setup issue remains separate and is not an a97 validation failure.

## Latest reviewed merge successor: `704c4f611c3586073e1efb322ae6fcd60c013f90`

- Tree: `d500af2a475db164a51b9a0211e48b2a01aeb29a`
- Parents: candidate `008fc842cf934d8e0270e0fa79a770958398ba4f`, current main `a84449ad6cbbe026f838cab656ad82993ccdc5e3`
- Scope: exact merged admission/reveal seams for PR #7817 and the existing #7794 expansion memo / token confirmation behavior.
- Disposition: `REPAIR_REQUIRED` due to a compile-blocking duplicate field in one reveal unit-test initializer.

### Finding

**[P1] Remove the duplicate implicit-observer field from the refusal regression initializer.** In `crates/ripr/src/analysis/classify/reveal.rs:7274-7277`, one `ReturnOracleAdmission` literal sets `assertion_has_known_implicit_observer` to `Some(false)` and then sets the same field again to `None`. `v4-reveal.receipt` binds this exact commit/tree and `v4-reveal.log` reports `E0062` at line 7277. Preserve the `Some(false)` callback because this test verifies the reported refusal behavior; remove the second, mechanically inserted neutral default. Re-run the exact-head reveal tests and the full v4 sequence after repair. Confidence: High.

### Other seam review

The merged main-side sibling exclusion is limited by `error_construction_path` to ErrorPath, ReturnValue, and FieldConstruction. For SideEffect/CallDeletion it therefore leaves the assertion as the original `OracleFact` before the implicit-observer callback, preserving kind refusal and the separate token-confirmation gates. The new `owner_shadowed` and `sibling_exclusion_admitted` checks only govern this error-variant sibling-exclusion path. The changed indexed dispatch helper and `ImplicitExpansionContext` remain unchanged from `008fc842`; the current `RelatedTestCandidateIndex` is built from and reused with the same `RustIndex`. The main selected-call refactor removes/moves targeted call-shape logic but does not change `test_dispatches_trait_impl` or its body-based `method_call_resolves_to_impl` witness. No other source interaction found in the reviewed merge seam.

Observed v4 evidence at review time: `v4-public.receipt` binds `704c4f61` / `d500af2a` and exits 0. `v4-reveal` fails during compilation on the duplicate field, so no reveal tests ran; further v4 steps were not established. Earlier `008fc842` exact-head and local-composition passes remain tied to their old identities and cannot qualify this merge successor. This is a local candidate review; hosted PR checks and review are still not observed. No Cargo was run by the reviewer, and no source was modified.

## Latest reviewed successor: `008fc842cf934d8e0270e0fa79a770958398ba4f`

- Tree: `092e0ffba298fce23da232be8abcf57ef3f3238f`
- Prior exact candidate: `8b251a29ff5b138ad165548e965eab976a33489c` (tree `657185158c06b36262a13ff7c279635e18e69276`)
- Scope: full #7794 seam currentness plus the `ImplicitExpansionContext` plumbing refactor and `is_drop` let-return cleanup.
- Result: no actionable source findings. Local candidate and composition proof is complete. Disposition before publication: `REVIEW_INCOMPLETE` because hosted PR qualification is not yet available.

The successor's context wrapper holds borrowed references to the same run's `RustIndex` and `ImplicitExpansionMemo`, and the only construction in classifier evidence pairs `context.index` with the memo attached to its `RelatedTestCandidateIndex` (or a fresh fallback memo). Its consumers use the paired context, removing the separate index argument from the admission path. The candidate index is built from the same index in the Rust adapter and reused for that classification pass. The `is_drop` simplification is a direct expression of the same predicate and does not change the fail-closed scan. No token-confirmation, Drop/import, macro, parse-failure, receiver-exclusion, or memo-cache predicate changed in this plumbing successor.

Observed exact-successor receipts: `v3-public` 14/14, `v3-reveal` 114/114, `v3-expansion` 2/2, `v3-transitive` 66/66, `v3-native-roots` 5/5, and `v3-cli-rerun` 17/17 all bind `008fc842` / tree `092e0ffb` and exit 0. `v3-goldens`, `v3-honesty`, `v3-dogfood`, `v3-check-fast`, and `v3-precommit` also bind that exact identity and exit 0. The check-fast log reports 15 gates with base `origin/main` and 55 selected files; comparing its retained 55-path selector input against `git diff --name-only ef2bce90...008fc842` produced no differences.

Observed composition: commit `6a089607056ea1643e11df587018aa7503347491` has tree `8454f317424e433cd12ad9a7c0eb7643313f2445` and parents `1cdc7ef41a7f17b3b6d3536582b01a5e46952545` (main) and `008fc842` (candidate). In that composed tree, `implicit_effect.rs`, `transitive_reach.rs`, and `sibling_effect_oracle.rs` are byte-identical to the candidate. `reveal.rs` and `classifier/evidence.rs` include main-side changes for predicate proximity, inline expect snapshots, unselected guards, and format-tail witnesses; source inspection shows those additions scoped to Predicate/ReturnValue or separate witness handling and do not alter SideEffect/CallDeletion confirmation. The composition receipts bind `6a089607` / tree `8454f317`: public 14/14, reveal 114/114, expansion 2/2, transitive 66/66, native roots 5/5, CLI rerun 17/17; goldens and evidence-promotion honesty exit 0.

Observed fixture-level native controls under `token-native-controls/` are separate from analyzer mutations: both original Rust fixtures compile and run 2/2; each write-removal variant compiles, then exits 101 because `implicit_observer` asserts 0 vs 1 while `direct_smoke` passes. These establish that the fixture assertion detects removal of the changed sink write, not that the analyzer itself was mutated. The reviewed workspace remained on `008fc842`; no Cargo was run during this review. Hosted PR checks/review are not yet observed, and #7880 remains an external corpus setup issue outside this claim.

## Reviewed successor: `8b251a29ff5b138ad165548e965eab976a33489c`

- Tree: `657185158c06b36262a13ff7c279635e18e69276`
- Parent reviewed candidate: `9660207b355688edc8ed595e38ab38ded61b8987`
- Scope: seven-line `ExpansionSurface::scan` tightening for renamed `Drop` imports; full #7794 seam disposition refreshed against this successor.
- Result: no actionable source findings. Candidate disposition: `REVIEW_INCOMPLETE` pending exact-successor focused/public proof and later hosted PR qualification.

The successor adds a per-`UseTree` path-tail check for `Drop`, catching forms such as `use std::ops::Drop as Destructor` that whole-item token compaction turns into `DropasDestructor`. It restores the prior conservative workspace veto for aliased `Drop` imports without changing alias support or receiver-specific custom-impl exclusions. The check can conservatively refuse a similarly named imported `Drop`; that is consistent with the existing fail-closed policy. It does not alter the `Some(false)` token-confirmation gates or the memo's per-immutable-`RelatedTestCandidateIndex` lifetime.

## Prior exact candidate: `9660207b355688edc8ed595e38ab38ded61b8987`

- Tree: `3ec0a540469050a2434334e9f68e2c75b5350af4`
- Prior source review found no actionable defects in independent token confirmation, expansion-gate equivalence, or run-scoped memo ownership.
- The successor-expansion change specifically repairs a counterexample against that prior candidate: `exact-expansion.receipt` binds the prior SHA/tree and records exit 101 because renamed `Drop` did not veto; `successor-expansion.receipt` binds this successor SHA/tree and records exit 0, with both expansion memo tests passing.
- `exact-public.receipt` (14/14) and `exact-reveal.receipt` (114/114) also bind `9660207`, not this successor, so they are not exact-successor evidence.

## Inspected surfaces and residual proof

Inspected `implicit_effect.rs` cached expansion scan, its two unit tests, the `Some(false)` confirmation branch in `reveal.rs`, candidate-index construction/lifetime, and the existing cross-file / aliased Drop regression matrix in `sibling_effect_oracle.rs`.

Observed evidence: successor expansion unit suite passes (2/2) at the exact successor identity. The review did not run Cargo. Exact-successor focused/public validation beyond this unit suite, composition at the updated main basis, remaining goldens/honesty/dogfood/check-fast/precommit proof, and hosted PR checks/review are not established here. Earlier main composition identity was for `9660207` and is stale after this successor edit.

Disposition: `REVIEW_INCOMPLETE` until the outstanding exact-successor proof is retained and then hosted qualification is observed on the published PR head.
