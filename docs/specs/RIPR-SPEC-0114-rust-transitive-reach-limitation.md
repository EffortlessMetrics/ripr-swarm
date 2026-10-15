# RIPR-SPEC-0114: Rust Transitive-Reach Limitation

Status: accepted

Owner: product / swarm

Created: 2026-06-15

Linked issues:

- dtolnay/semver dogfood: `pub(crate) matches_greater` reached only via `VersionReq::matches`
  (pub → pub(crate) helper chain) — ripr returned bare `no_static_path` with empty stop_reasons,
  misleading new users into thinking tests are absent.

Linked PRs:

- [#1305](https://github.com/EffortlessMetrics/ripr-swarm/pull/1305) -
  implemented the fail-closed Rust transitive-reach limitation.

Support-tier impact:

- No tier change. This spec adds an additive optional JSON field (`static_limit_kind`) and a
  named stop-reason (`transitive_reach_unresolved`) to `no_static_path` findings where a bounded
  BFS walk detects a plausible transitive path. Classification stays `no_static_path` (fail-closed);
  no promotion to weakly_exposed or higher.
- The `static_limit_kind` field is already defined in the schema; `rust_transitive_reach_unresolved`
  is a new enum value (additive). No `schema_version` bump is required.
- Claim boundaries remain governed by the canonical ledger in
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- Register `RustTransitiveReachUnresolved` in `crates/ripr/src/domain/language.rs`.
- Register `TransitiveReachUnresolved` in `crates/ripr/src/domain/probe.rs`.
- No new crates, binaries, dependencies, parsers, runtime executors, or LSP servers introduced.

## Problem

When ripr classifies a Rust finding as `no_static_path`, it means the direct-call classifier
found no test that statically calls the changed owner by name. But many real-world production
functions are `pub(crate)` helpers only reachable through a public façade. A test calls the
façade (`VersionReq::matches`), which calls the helper (`matches_greater`), which is the changed
owner — but ripr's direct-call classifier only checks one hop. The result is bare `no_static_path`
with empty `stop_reasons`, which looks the same as "genuinely untested."

This slice names the limitation honestly:

> ripr saw a test reaching public API that may call toward this change through a transitive path
> it does not fully trace (pub to pub(crate) helper chains, macros, or generics). This is not a
> coverage assessment — ripr cannot confirm or deny that the change is observed.

## Behavior

### Trigger condition

After the direct-call classifier returns `ExposureClass::NoStaticPath` with empty `related_tests`
(or `WeaklyExposed` with `weak` reach and only proximity relations, #7071; see RIPR-SPEC-0117),
the Rust adapter runs a bounded BFS walk over the lexical call facts in the RustIndex:

1. Collect all tests from the index (unit + integration).
2. For each test's `CallFact` that is NOT a macro invocation and NOT a direct call to the owner:
   find every production `FunctionFact` in-crate with that name (name-only facts cannot
   tell same-named methods of different types apart, so the walk follows each of them).
3. BFS from that production function through `FunctionFact.calls`, depth ≤ 5:
   - Stop at macro invocations (`name!`).
   - Stop when callee name not found in-crate (external / unresolved).
   - Stop when depth > 5.
4. If the owner's name is reached within depth ≤ 5: a candidate path was found.
5. Only when step 4 names no witness, the same walk continues past depth 5,
   level by level (#8047). At each deeper level a test may enter (a test
   calls one of its names, or a derive entry exists), the witness selection
   of step 4 runs over every name reached so far; the first witness ends
   the walk, and a level whose only "callers" are test declarations
   (`fn name()` facts) yields none, so the walk goes on. A chain of any
   length that a test enters is still a path ripr does not trace, so it
   names the same limitation; a chain no test enters still finds nothing.
   The walk runs only for an owner a test or a test-entered function calls
   at any depth (one forward closure per index), so an owner in code no
   test enters costs nothing. It is the last reach witness every consumer
   tries: after the bounded walk, and after the macro (RIPR-SPEC-0117),
   subprocess, property-macro and seam trait-dispatch witnesses. A deeper
   name-only chain therefore never changes the limit or witness a
   shallower or more specific path names.

### When found

Set `Finding.static_limit_kind = Some(StaticLimitKind::RustTransitiveReachUnresolved)`.
Push `StopReason::TransitiveReachUnresolved` to `Finding.stop_reasons`.
Push the honest limitation message to `Finding.evidence`.

The classification **stays `no_static_path`**. This is a named limitation, not a reach promotion.

RIPR-SPEC-0118 refines the wire kind for witnesses that start in integration
test paths: those use `rust_integration_public_api_path_unresolved` while
non-integration helper chains keep `rust_transitive_reach_unresolved`. The stop
reason, classification, witness language, and fail-closed boundaries remain
unchanged.

#7071 extends the trigger to `weakly_exposed` findings whose related tests are
all proximity-only; RIPR-SPEC-0117 states the rule. The class stays
`weakly_exposed`.

### When NOT found

Leave the finding exactly as today (bare `no_static_path`, or a proximity-only
`weakly_exposed` finding with its gap, and no `static_limit_kind`).
Genuinely-untested stays genuinely-untested.

### Fail-closed boundaries

- NEVER change the classification from `no_static_path` (or from `weakly_exposed` on the
  proximity-only path, #7071). No promotion of any kind.
- Walk is NAME matching over LEXICAL call facts only — no AST resolution, no visibility tracking.
- The witness walk is bounded at MAX 5 hops. Past it, the walk only asks
  whether a test enters the chain at any depth, and names the same
  limitation when one does (#8047); it never ranks a deeper witness above a
  shallower one.
- Macro invocations (`name!`) stop the walk immediately at that branch.
- Callees not found in the production function set stop that branch (fail-closed on external deps).
- The message uses "may" language: "may call toward this change through a path ripr does not fully trace."

### Wire format

| Field | Value when fires | Value when not fires |
|-------|------------------|----------------------|
| `classification` | unchanged (`no_static_path`, or `weakly_exposed` when proximity-only) | unchanged |
| `static_limit_kind` | `"rust_transitive_reach_unresolved"` | omitted / null |
| `stop_reasons` | includes `"transitive_reach_unresolved"` | unchanged |
| `evidence` | includes the limitation message | unchanged |

## Non-Goals

- Promoting `no_static_path` to `weakly_exposed` or higher — that is the next P3 slice.
- Tracing macro invocations or generic dispatch — explicit fail-closed boundary.
- Tracking visibility (pub vs pub(crate)) — not available in lexical call facts.
- Reach credit through a chain of any depth. A path past 5 hops names the
  limitation only (#8047); the classification never changes.

## Acceptance Examples

RIPR-SPEC-0118 refines the two integration-test examples below to
`static_limit_kind: rust_integration_public_api_path_unresolved`. Non-integration
transitive helper witnesses keep `rust_transitive_reach_unresolved`.

1. **Positive (limitation fires)**: `pub(crate) fn inner()` changed; integration test calls
   `outer()` which calls `inner()`. Direct-call classifier finds no test for `inner`. BFS finds
   `test → outer → inner`. Result: `no_static_path` + `static_limit_kind: rust_transitive_reach_unresolved`.
   Fixture: `fixtures/rust_transitive_reach_positive/`.

2. **Test-helper public API chain (limitation fires)**: a semver-shaped integration test calls
   a test helper (`assert_public_api_accepts`), the helper calls a public API function, and the
   API flows through internal helpers to the changed owner (`matches_greater`). Result:
   `no_static_path` + `static_limit_kind: rust_transitive_reach_unresolved`, with a witness naming
   the helper entry point. Fixture: `fixtures/rust_transitive_reach_test_helper_chain/`.

3. **Negative (limitation does NOT fire)**: `apply_fee` changed; no tests at all. BFS finds no
   candidate path. Result: bare `no_static_path`, no `static_limit_kind`. Classification unchanged.
   Fixture: `fixtures/rust_transitive_reach_negative/`.

4. **Fail-closed on macros**: test calls `my_macro!()` which calls the owner — BFS stops at macro.
   No limitation fires (no candidate path found through macros). Classification stays bare `no_static_path`.

5. **Depth boundary**: test → fn_a → fn_b → fn_c → fn_d → fn_e → fn_f → owner (6 hops) —
   exceeds the 5-hop witness walk, so the walk continues past it and finds the
   test entering the chain at `fn_a` (#8047). Result: the same limitation as
   example 1, naming that test. Classification unchanged (`no_static_path`, or
   `weakly_exposed` when proximity-only). The verdict corpus row
   `tokens-base-six-hop-chain` (a proximity-only `weakly_exposed` owner six helper
   hops from its only pinning test) reads `limited`. A 6-hop chain no test
   enters still finds nothing.

6. **Existing tests found directly**: direct-call classifier finds a test for the owner (related_tests
   non-empty, with at least one relation that is not proximity-only). BFS does NOT run. Finding classified normally (infection_unknown or higher).

## Required Evidence

- `StaticLimitKind::RustTransitiveReachUnresolved` added to `domain/language.rs`.
- `StopReason::TransitiveReachUnresolved` added to `domain/probe.rs`.
- `analysis/classify/transitive_reach.rs` module with BFS walk, bounded at depth 5, fail-closed on
  macros/externals.
- Wired in `analysis/language/rust/mod.rs` `analyze_diff` and `analyze_repo` post-classify guards.
- Positive fixture: `fixtures/rust_transitive_reach_positive/` — limitation fires.
- Test-helper public API fixture: `fixtures/rust_transitive_reach_test_helper_chain/` — limitation fires.
- Negative fixture: `fixtures/rust_transitive_reach_negative/` — limitation does NOT fire.
- Unit tests covering: positive path, no path, depth-5 boundary (found), depth-6 (found by
  the walk past the bound, not found when no test enters the chain), a witness within the
  bound winning over a deeper one, the deeper walk running after the macro witness, macro
  skip, external callee stop, honest message language.
- JSON evidence and human output surface the limitation detail: last established
  edge, first unresolved edge, analyzer route, and non-claim.

## Test Mapping

- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_test_calls_outer_which_calls_owner_then_witness_is_captured`
  — positive path found
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_no_path_to_owner_then_witness_is_none`
  — no path, limitation does not fire
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_path_at_depth_5_then_witness_is_captured`
  — depth 5 boundary: still finds
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_path_depth_6_then_the_walk_past_the_bound_names_a_witness`
  — depth 6: the walk past the bound names the entering test; a test that enters no
  chain name still finds nothing (#8047)
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::a_witness_within_the_bound_wins_over_a_deeper_one`
  — a witness within 5 hops is named before the deeper walk runs (#8047)
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::the_walk_past_the_bound_skips_levels_no_test_enters`
  — an 8-hop chain entered only at the top is found; a back-edge cycle neither loops nor
  invents a witness (#8047)
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::a_test_declaration_on_a_deep_level_does_not_end_the_walk`
  — a test named like a deep chain function does not stop the walk short of a real
  caller (#8047)
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::seam_behind_a_helper_chain_past_five_hops_is_opaque_not_ungripped`
  — the same witness keeps a repo seam `opaque` (RIPR-SPEC-0230 rule 1, #8047)
- `crates/ripr/src/analysis/language/rust/proximity_reach_tests.rs::a_chain_past_five_hops_does_not_displace_the_macro_witness`
  — a one-macro-hop witness stays named beside a seven-hop chain; without the macro, the
  chain names the transitive limit (#8047)
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::seam_reach_past_five_hops_does_not_displace_a_macro_witness`
  — the walk past the bound is also the last seam reach witness (#8047)
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_macro_call_in_test_calls_then_macro_is_skipped`
  — macro invocations skipped
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_callee_not_in_crate_then_walk_stops_fail_closed`
  — external callees stop walk
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::wire_message_contains_honest_may_language_and_no_coverage_claim`
  — message uses "may", no coverage claim
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::transitive_reach_limitation_detail_names_edges_route_and_non_claim`
  — JSON evidence names the last established edge, first unresolved edge, analyzer route, and non-claim
- `crates/ripr/src/output/human.rs::tests::human_output_surfaces_static_limitation_detail`
  — human output projects limitation detail under `Limitation detail`

## Implementation Mapping

| Component | Location |
|---|---|
| `StaticLimitKind::RustTransitiveReachUnresolved` | `crates/ripr/src/domain/language.rs` |
| `StopReason::TransitiveReachUnresolved` | `crates/ripr/src/domain/probe.rs` |
| BFS walk module | `crates/ripr/src/analysis/classify/transitive_reach.rs` |
| Module export | `crates/ripr/src/analysis/classify/mod.rs` |
| Wiring (diff mode) | `crates/ripr/src/analysis/language/rust/mod.rs::analyze_diff` |
| Wiring (repo mode) | `crates/ripr/src/analysis/language/rust/mod.rs::analyze_repo` |
| JSON evidence detail | `crates/ripr/src/analysis/classify/transitive_reach.rs::transitive_reach_limitation_detail_lines` |
| Human renderer detail | `crates/ripr/src/output/human/sections.rs` |
| Positive fixture | `fixtures/rust_transitive_reach_positive/` |
| Test-helper public API fixture | `fixtures/rust_transitive_reach_test_helper_chain/` |
| Negative fixture | `fixtures/rust_transitive_reach_negative/` |

## CI Proof

- `RUSTFLAGS="-D warnings" cargo build -p ripr -p xtask` — exit 0.
- `cargo test --workspace` — all pass including new BFS unit tests.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.
- `cargo xtask check-static-language` pass (limitation uses "no_static_path", not forbidden vocabulary).
- `cargo xtask check-architecture` pass.
- `cargo xtask check-no-panic-family` pass.
- `cargo xtask check-allow-attributes` pass.
- `cargo xtask check-doc-artifacts` pass.
- `cargo xtask check-doc-index` pass.
- `cargo xtask check-spec-format` pass.
- `cargo xtask check-traceability` pass.
- `cargo xtask goldens check` clean (no existing no_static_path classifications changed).
- `cargo xtask fixtures` clean (both new fixtures pass).

## Metrics

- Gate: 0 golden drift on existing fixtures; 3 transitive-reach fixtures passing.
- RIPR-SPEC-0118 supersedes the integration-origin fixture wire kind with
  `rust_integration_public_api_path_unresolved`.
- Behavioral repro: positive fixture shows `static_limit_kind: "rust_transitive_reach_unresolved"`,
  classification stays `no_static_path`. Negative fixture shows bare `no_static_path`, no `static_limit_kind`.
