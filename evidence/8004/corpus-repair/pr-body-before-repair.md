## Summary

Refs #7415. Require bounded receiver evidence before attributing a direct call to a parser-established inherent method with `self`. A unique indexed method name is insufficient: a private xtask `JsonValue::as_object` can share its spelling with unrelated dependency methods in both the main and xtask targets.

The diff and repository/compact consumers share a deny-only refinement based on actual call syntax and the nearest visible simple binding. Supported assertion operands are parsed in their original invocation scope. Comment-only, expired-binding and unsupported pattern/item scopes supply no receiver proof. Real object/null and public-library calls retain attribution.

The helper graph also refuses name-token self edges: function declarations otherwise let wrong receivers re-enter as HelperOwnerCall after direct admission refuses them. Genuine helper relations remain admissible.

### Follow-up repair

An independent adversarial review found an operand-local unit struct named `value` shadowing an outer `JsonValue` variable. Its foreign blanket-trait call was incorrectly direct/high. The repaired guard refuses all operand-local AST items and exempts only the actual synthetic root function node, never a same-named nested function. Regression matrices, native controls, a three-surface fixture and honesty corpus cover this defect and related item binders.

## Exact candidate and review

- Published follow-up head: `15ca898ad1d3571d104ce1ae20b629fe2f55fc86`.
- Independently reviewed local head: `def018d1a67109fa327f094cd63c38b936a407e0`.
- Both have tree `a8c022a078c40285c8e38624c5c0431f9e907531` and sole parent `f6aab7639b88be1a21c757be74baef833e636f17`; independent tree/parent/empty-diff bridge verified. This is source identity, not a new executable stamp.
- Follow-up delta: 16 paths; total PR delta: 19 paths from merge-base `89ac47a603092ba7f9f55cf74999b88649621abf`.
- Frozen executed current-main composition: `b960feed712cbb71e6d07c9f902e057a677261c3`, tree `159f28028a3e91cd4256fa2b95d0c65c2d5c5354`, parents `2946fc9be09dd1e4f2a248c209561f718aa976df` + reviewed `def018d1`. Substituting published bridge `15ca898` produces the same tree.
- Later observed main `5469089b81d7a59d7059e2b8aab64d109cc9c095` adds comparison-connective infection handling. Reviewer inspected the delta: relevant receiver owner/consumer blobs are unchanged and the concrete receiver fixture has MatchArm/ReturnValue probes, not the affected guarded-predicate edits. Textual composition succeeds as tree `fe0852322f88ab4a4aa79f61ebf4bf5b00c68061`; actual execution remains bound to the earlier frozen composition.

## Discriminating proof

- Compiling original receiver/type-collision controls reject both wrong main/same-target receivers while retaining real private/public callers and a genuine mixed helper.
- The operand-struct native witness passes three subjects; changing only the private Object branch makes the genuine Object subject fail while the foreign-trait decoy still passes.
- A new production test executes and fails on the old guard's DirectOwnerCall, then passes after repair. Six production integration tests pass, including nine scope/item variants.
- Clean final author head: two full/compact relation units pass, 10,814 filtered. Its unit build stamps `def018d1`, dirty=false.
- Author check-fast: all 15 selected gates pass, including full workspace/all-target Clippy with `-D warnings`. Independently checked 19-path three-dot selector against frozen origin/main `0e8d006`; no skips or zero-selection shortcut.
- Independent reviewer: 10 fixtures across diff/detailed/compact surfaces, 30 passing reports with nonempty subject and relation/metric checks; native 10/10 pass. A real-owner mutation yields 9 pass/1 fail, with all eight decoys surviving.
- Clean current-main composition: two relation units pass (10,844 filtered), six production receiver integration tests pass, targeted cli_smoke rerun selection passes 17 tests (502 filtered), native_path_roots passes five. Rust/Cargo 1.95, default features, locked/offline, one job, incremental disabled, dev/test debug 0. These are selected tests, not a full-default-suite pass.
- The clean composed CLI stamps `b960feed`; the reviewer replayed all 30 independent reports on it. The earlier pre-commit CLI remains explicitly `f6aab763-dirty`, source-equivalent by manifest, rather than being relabeled as a final-head build.
- New fixture/corpus and applicable documentation/traceability/changelog checks pass; an intentionally wrong DirectOwnerCall corpus rebless is rejected.

An initial new unit oracle incorrectly required retention of an unrelated cross-target row. That test-oracle failure and its bounded correction remain retained; it is not a production guard failure or an erased attempt.

## Hosted qualification and remaining boundaries

Current-head hosted qualification is separate from local proof. Historical `f6aab763` Rust-only CI passed, but its UB-review gate was permissive: zero required sensors/proofs and two optional failures. Default cargo-test failed canonical fixture dependency setup (#7920); installed RIPR 0.10.0 refused analysis because 1,229 indexed files exceeded 800 (#7903/#7915). Those are not clean test/analysis results or latest-head required-check clearance. No gate, limit, installer pin or quarantine is relaxed here.

The earlier 425-fixture golden check found one semantic drift and its two human renders for `owner_return_pin_fallible_literal`; immutable baseline `89ac47a6` reproduced the exact normalized JSON. That historical baseline comparison and SIGKILL attempts remain in retained evidence. This repair does not claim a new full-golden, dogfood or full local precommit run.

The unresolved parse-memoization review nit is tracked separately in #8005. Its measurement/prototype is not part of this source repair; no CI speedup or all-threads-resolved claim is made.

This is not complete Rust type/Cargo-target identity. Same-terminal foreign types, constructor conventions, aliases/factories/parameters, general value flow, unsupported macro expansion and helper last-hop identity remain bounded limitations under #7415. Separately owned #7817 and #7973 branches are not qualified by this composition. #7415 stays open.

## Retained evidence and continuation

- [Original source/review packet](https://github.com/EffortlessMetrics/ripr-swarm/tree/fd044edea302d38fba7b8e6e8ba81cee9a78f804/evidence/8004/receiver-attribution).
- [Frozen composition, hosted gaps and confirmed operand-shadow red](https://github.com/EffortlessMetrics/ripr-swarm/tree/7dafbebe35d5e3c2cd0b66331969cb300623d463/evidence/8004/followup).
- [Final repaired source/review/current-main qualification packet](https://github.com/EffortlessMetrics/ripr-swarm/tree/faff142b4607c65bf02e431a73562ad21ba23766/evidence/8004/item-guard), with checksums and preserved failed attempts.

Owner: Rook. Phase: WAITING_REQUIRED_CHECKS at exact published `15ca898`. The independent published-head source/integration review cleared Ready-to-start-checks; Ready was verified on 2026-10-15 at 00:32 UTC. Required Routed Rust Small run [38660627400](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38660627400) is admitted; current-head CI and UB-review runs are also in progress. Root owns terminal-result/actual-checkout reconciliation and any scoped repairs. Ready starts validation; it does not grant merge clearance. Merge review remains REVIEW_INCOMPLETE pending current-head required checks, review-thread disposition, dependency/ownership and protection requirements. No release, deployment or UB-review adoption is requested.

The actual published merge object was independently fetched: `2fa3df5b4eca7cfb8376a676b2380a59aaf02c35`, tree `d1c80eaa9192ca99b11011221bc80d0b342f9010`, parents `9543a9bd733b0921886306abd2c2609fbe73b62e` + `15ca898`. Main `9543a9b` adds only packaged-README xtask/release-documentation work after `5469089`; relevant receiver source remains unchanged. This verifies the merge object, not a hosted job checkout. API PR-base metadata still reports older `0e8d006` and is not used as checkout authority.

## Changelog and rollback

`changelog.d/7415-inherent-receiver-admission.md` uses the canonical Fixed section, bullet and trailing issue reference; the operand-item refusal is included. No exemption or deferred changelog entry.

Rollback boundary: revert this bounded receiver-admission/self-edge change and its associated tests/fixture/docs together; doing so restores the demonstrated attribution defect. Do not treat rollback as evidence of correct receiver identity.


