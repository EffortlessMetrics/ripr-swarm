# `ripr/listActionableItems` mutation receipt (#3032)

Date: 2026-11-02
Repository: `EffortlessMetrics/ripr-swarm`
Inspected ref: `origin/main` at `c067b5491c2ba924881499aa82b366bede0d60b7`
Classification: `verified_current`

## Scope

Issue #3032's residual acceptance row: a retained removal/mutation receipt
proving the tests landed by #3031 (`6fc4d5eb`) fail when one handler branch or
capability field of the `ripr/listActionableItems` slice (#3012, squash
`d3fafbcb`) is weakened. Each mutation below makes behavior wrong while
keeping the crate compilable; each was applied alone, measured, and reverted
before the next ran.

PR #3133 previously retained this receipt but was reverted by the #3140 release
scope recovery; this document re-runs the experiment on current main.

## Baseline (unmutated `origin/main`)

```text
cargo test -p ripr --lib lsp::backend::list_actionable_items_tests
  (four branch tests: without_snapshot, analysis_in_flight, unavailable, applied — all ok)
  test result: ok. 12 passed; 0 failed
cargo test -p ripr --lib lsp::agent_protocol
  test result: ok. 23 passed; 0 failed
```

## Mutation A — handler error branch weakened

Change: in `Backend::ripr_list_actionable_items`
(`crates/ripr/src/lsp/backend.rs`, no-snapshot branch), `"kind": "no_snapshot"`
replaced with `"kind": "analysis_in_flight"`.

Expected: `list_actionable_items_without_snapshot_fails_closed` fails; the other
three branch-path tests still pass.

Actual:

```text
cargo test -p ripr --lib lsp::backend::list_actionable_items_tests  (exit 101)
  list_actionable_items_without_snapshot_fails_closed ... FAILED
  list_actionable_items_analysis_in_flight_fails_closed ... ok
  list_actionable_items_unavailable_delivery_fails_closed ... ok
  list_actionable_items_applied_returns_bounded_envelope ... ok
  test result: FAILED. 10 passed; 2 failed
```

The collateral failure is `list_actionable_items_interim_response_shape_is_closed`,
which also keys off the no-snapshot error kind — still branch-discriminating.

Reverted with `git checkout -- crates/ripr/src/lsp/backend.rs`.

## Mutation B — success envelope weakened

Change: the `"must_not_change": [...]` field removed from the applied-outcome
envelope in `Backend::ripr_list_actionable_items` (line ~6049).

Expected: `list_actionable_items_applied_returns_bounded_envelope` fails; the
three error-path branch tests still pass.

Actual:

```text
cargo test -p ripr --lib lsp::backend::list_actionable_items_tests  (exit 101)
  list_actionable_items_applied_returns_bounded_envelope ... FAILED
  list_actionable_items_without_snapshot_fails_closed ... ok
  list_actionable_items_analysis_in_flight_fails_closed ... ok
  list_actionable_items_unavailable_delivery_fails_closed ... ok
  test result: FAILED. 10 passed; 2 failed
```

Collateral failure: `list_actionable_items_interim_response_shape_is_closed`
(missing `must_not_change` in the closed shape).

Reverted with `git checkout -- crates/ripr/src/lsp/backend.rs`.

## Mutation C — capability field weakened

Change: in `RiprAgentCapability::v0_1_implemented()`
(`crates/ripr/src/lsp/agent_protocol.rs`), `supported_requests` changed from
`vec![RiprAgentRequest::ListActionableItems]` to `vec![]`.

Expected: `capability_advertises_only_implemented_handlers` fails.

Actual:

```text
cargo test -p ripr --lib lsp::agent_protocol  (exit 101)
  capability_advertises_only_implemented_handlers ... FAILED
  capability_output_matches_the_published_schema ... FAILED
  test result: FAILED. 21 passed; 2 failed
```

Reverted with `git checkout -- crates/ripr/src/lsp/agent_protocol.rs`. After
all three reversions `git status --porcelain` is empty.

## Verdict

Each mutation was caught by tests pinned to the weakened branch or capability
field (plus closely coupled shape/schema guards). The #3031 tests discriminate
handler branches and the capability contract as #3032 requires. This receipt
satisfies #3032's residual acceptance row (removal/mutation experiment). It
does not satisfy #3080's separate, stronger removal experiment over typed
envelopes and immutable snapshots, which remains owned by #3080.

## Companion evidence: `MutexGuard::clone` claim refuted

The #3012 post-merge review raised a possible `MutexGuard::clone` type defect
in the landed snapshot extraction. Refuted on current main: `guard.clone()` on
`MutexGuard<Option<Arc<AnalysisSnapshot>>>` resolves through `Deref` to
`Option::clone` of the inner `Arc`; #3031's `.and_then(|guard| (*guard).clone())`
makes that explicit. See PR #3031 disposition and #3133 discussion.
