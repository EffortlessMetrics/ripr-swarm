# Independent candidate review: bounded inherent receiver admission (#7415)

No blocking source, oracle, or scope findings emitted. Semantic review of this exact local candidate is complete. Publication and hosted merge qualification remain outside this pass.

reviewed_head_sha: 6b382125783a05e9b3353131837ed215fce70247
reviewed_tree: 81af6df65693db067d08e661ea0f33b3ef41342c
integration_basis: parent 89ac47a603092ba7f9f55cf74999b88649621abf
review_date: 2026-10-14 UTC
claim_boundary: parser-established inherent methods with self no longer receive direct/high relations from unique indexed names alone; deny-only nominal plus real expression/visible local-binding admission applies in diff and full/compact repository consumers. No general Rust type or Cargo-target resolver claim.

## Inspected owners and consumers

Inspected the complete nine-file committed diff and applicable repository review and validation context. Followed the shared admission into owner-call matching, helper graph construction and fallback priority, full and compact evidence generation, public relation projections, the spec, changelog and traceability.

The guard applies only to parser-established inherent owners with self. Trait/default dispatch and no-self associated functions keep their prior admission. Lexical/type uncertainty withholds direct credit. Whole-test comment masking precedes syntax parsing; assertion operands use the original invocation scope and omit diagnostic arguments. Expired bindings, relevant pattern scopes and scope items cannot borrow an outer same-spelled value. The compact graph no longer records the declaration-name self edge. Genuine separate helper routes still run after refused direct matching.

## Independently executed proof

Execution used a reviewer-owned detached checkout; no submitted-candidate edits, GitHub actions, branch moves, pushes, merges or releases. The checkout was clean before and after execution, and head/tree/parent were re-read before this record.

- Rust 1.95.0 / Cargo 1.95.0, Linux x86_64; locked offline build, no-default-features plus lang-rust, jobs=1, incremental=0, dev/test debug=0.
- Exact-head private_inherent_receiver integration suite: 6 passed, 0 failed, 0 ignored. Includes both enum positives, serde decoys, public inherent integration, raw call-line comment, expired binding, while-let, scope items and assertion-operand items.
- native_path_roots: 5 passed, 0 failed, 0 ignored. Three real CLI root/cache journeys plus two helper tests on Linux; this does not establish native Windows behavior.
- Independent compiling native control fixture: 15 executed subjects, consisting of main 1, explicit-path xtask 13, public integration 1. The zero-test library harness is excluded from this denominator.
- Ten isolated controls each compiled and ran 3 native subjects and then ran diff JSON, full repository JSON and compact summary JSON. All 30 reports exited 0; each diff was complete with 3 probes/findings and each repository report complete with 5 seams. Isolation kept related-row counts below output caps. Literal positive expectations held for UFCS, borrowed/parenthesized receivers, ordinary statement calls and inline enum variants. Literal refusal expectations held for if-let, for, match and closure shadows, diagnostic-only calls and a local no-op assertion macro. Both private diff probes and all four private repository seams were checked. Full/compact class counts agreed for every case. Negative cases retained only 1 strongly-gripped public seam; positives retained all 5.
- Isolated wrong-only fixture: 5 native subjects passed; full/compact both retained 5 seams with 1 strongly-gripped public seam and 4 opaque private seams. Wrong serde calls had neither DirectOwnerCall nor HelperOwnerCall. This challenges compact helper re-entry independently of the true positives.
- Isolated mixed fixture: 6 native subjects passed; the only genuine private-owner route was a separate helper called beside the wrong serde direct call. Full repository rows retained HelperOwnerCall on all four private seams; full/compact both retained all 5 strongly-gripped seams. Diff rows stayed name-only for the wrong direct call, as the existing diff priority dictates; no general helper-transfer rewrite is claimed.
- git diff --check passed.

The actual selected Cargo binary is retained as ripr-6b382125-rustonly beside this record. Observed version: ripr 0.11.0 (6b382125783a05e9b3353131837ed215fce70247). SHA-256: 814c6373c78aad5acbb57a6c70084eb0b18a43fe3acadf5ee0207218f10172aa. review-receipt.json records the selection/build identity; control-results.json and isolated-results.json record literal invariants. Raw reports, native logs and exits are retained. Final verification used the correct report schema and has explicit terminal success; only verified reports are counted as proof.

## Non-blocking efficiency suggestion [P2]

Location: crates/ripr/src/analysis/classify/related_tests.rs:3044-3063; crates/ripr/src/analysis/test_grip_evidence/related_tests.rs:23-32, 257-278.

Source observation: inherent_receiver_call_resolves masks and reparses a nominally matching whole test, then may parse standard assertion operands. Both repository consumers call relationship matching once per seam, so seams of the same owner repeat the same owner/test decision. The compact caller then remasks and repeats nominal matching through method_call_resolves_to_impl. The existing CompactGripContext parsed_sources memo (context.rs:49-52, 708-713) does not cover this new guard. Diff matching also revisits the guard for multiple probes of the same owner.

Inference: this adds avoidable work proportional to seam/probe count times candidate tests and test-body/operand size. For the four private seams of the basic fixture, the two actual owner tests imply at least eight whole-test guard parses in each separate full or compact report, despite two distinct owner/test decisions. This is a source-derived count, not an instrumented measurement. No representative performance benchmark or measured CI slowdown was established.

Smallest follow-up: memoize the deterministic boolean admission by frozen index owner/test identity in the existing run/window contexts; cache refusals too. Preserve window cleanup, producer/currentness scope, and the same denial/positive semantics. This needs no global resolver or persistent cache. Existing parse nesting/operator budgets and the receiver unwrapping limit remain in force; no newly unbounded recursion was found. A future change should re-run these controls and add a bounded repeated-seam/call-count check.

## Missing proof and residuals

- No hosted checks, published PR head/merge ref or protections were inspected. This is candidate review, not merge readiness.
- Default-feature full lib-test execution was not repeated. The author's reported rustc SIGKILL remains an infrastructure gap; this independent pass provides Rust-only production integration and real CLI full/compact proof, not a full/default workspace-suite pass.
- Constructor naming conventions, aliases, foreign qualified types with the same terminal name, arbitrary receiver value flow/unsupported macro or pattern scopes and general helper last-hop identity remain bounded residuals. No complete #7415 closure or exact global attribution claim is earned.
- Trait-dispatch #7817 and mutable-field equality #7973 were not composed or qualified by this pass.

blocking_findings: none
non_blocking_suggestions: repeated parser work/memo opportunity above
refuted_or_stale_findings: no new claim; original unique-name false credit is rejected by the independent wrong-only witness
proof_or_review_missing: hosted/default-suite/integration dimensions above
affected_currentness_dimensions: candidate head, implementation, test stimulus/oracle, public claim, traceability inspected at the frozen object
residual_assumptions_and_non_claims: nominal admission only, separate branches uncomposed, Rust-only Linux proof

disposition: REVIEW_INCOMPLETE solely for publication/hosted qualification and the retained broader suite gap. No semantic blocker found for publishing this bounded candidate as Draft and proceeding through the owner's applicable integration/qualification route.
