REVIEW_READY

Substantive exact-head review of a232323fe744f322e2bdf77905aecc009490873b (head of #13432), resumed from a stale draft and re-composed with current main (66784c9c20) on 2026-11-08.

## Surfaces reviewed
- Core change is a genuine simplification-plus-correctness repair: the single-line `enclosing_bracketed_target_binds` textual heuristic (~70 lines that re-parsed one physical line and could not see multiline tuple targets) is deleted. In its place, `binds_in_target_list` falls back to `parse_test_function_scope` (the shared budgeted parser) plus a new `body_rebinds_name` scope-binding collector: a receiver bound more than once in its scope (tuple/list targets across physical lines, split compound headers, multiline loop/with targets, match-case captures, `global` declarations) withholds `protocol_entry_call` credit; tuple RHS values, call arguments, subscript indices, and dotted/class-pattern reads keep it. Fail-closed direction throughout: ambiguous parses withhold, malformed source never invents a suite.
- `visit_statements` is refactored into a `visit_statements_while` predicate visitor so the rebind collector visits scope definitions without entering nested function/class suites (outer-scope case captures stay attributed correctly, pinned by tests).
- Tests pin both directions: six multiline rebind shapes must lose credit (including `del x, (r)`), six multiline read shapes must keep it (including a triple-quoted string containing a fake binding and a commented-out binding — the parse-based approach reads real syntax only, which the old text scan could not guarantee). Match-case capture attribution gains positive and negative controls.
- Cache generations re-stacked onto current main after its `#[async_std::test]` admission (#5040): full `1.174`, sharded/compact `0.176` with paired comment entries recording both transitions and pinned assert updates. The 21 Rust-only honesty-corpus captures are pinned in `corpus.json` at their recorded generation (`1.173`, as main's #5040 lane regenerated them; this rule judges only Python seams, so a metadata-only churn was refused); `cargo xtask check-evidence-promotion-honesty` passes, and its regeneration guard independently re-ran every Rust subject at the current generation and confirmed judged seams, verdicts, stages and test relations unchanged. Spec RIPR-SPEC-0028 text updated to describe the new capability and the retained conservative misses (dict-key continuation line, same-line case guard) honestly.
- The `python_protocol_entry_bracketed_rebind_no_exposed_credit` fixture stayed intact through both re-compositions: four new multiline rebind subjects, related-test count 7→11, all still uncredited; check/human goldens updated through the producer.
- Inherited main-defect repair carried since head 2aea6e775d (verified failing on exact main base 2ac0c461c3 before the fix): #13668 settled the `module_reach_control_qualified` return-value probe through the shared call identity (#13625) and regenerated `expected/check.json` to `exposed`, but left the SPEC `expected-classes` block at `weakly_exposed`, so `check-fixture-contracts` (#12928) failed on main for any PR touching fixtures. The block now matches the intended class stated in the same SPEC and in the regenerated check.json; no corpus pin held the probe at `weakly_exposed`. Tracked as #13796.

## Validation observed on this head
- `cargo check --workspace --all-targets`: exit 0.
- `cargo xtask check-evidence-promotion-honesty`: pass (includes per-subject regeneration comparison at the current generation).
- `cargo xtask check-spec-format`, `check-traceability`, `check-fixture-contracts`, `check-output-contracts`: all exit 0.
- Review threads: 0 unresolved.

## Residuals
- The new fallback parses the complete test body per call site (bounded by the shared parser budget); hosted benchmark evidence remains the oracle for any perf delta, as with the original candidate.
- The documented dict-key and same-line case-guard conservative misses remain, as the body and spec declare.

Reviewed by the resuming session; ready for merge convergence.
