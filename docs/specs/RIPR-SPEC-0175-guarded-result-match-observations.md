# RIPR-SPEC-0175: guarded Result match observations

Status: proposed

Owner:

Created: 2026-09-10

Linked proposal:

Linked ADRs:

Linked plan:

Linked issues:

- #3709 (derive owner-bound guarded Result observations)
- #1528 (downstream qualification parent; the #13162 comparison)
- #3284 / RIPR-SPEC-0154 (the Err-return guard family this extends; that spec
  excluded match-arm Err forms as a non-goal)

Linked PRs:

Support-tier impact:

- No tier change. The admit widens which test-body shapes produce oracle
  facts for an already-analyzed probe; it adds no runtime evidence, no
  harness execution, and no new analysis surface.

Policy impact:

- `oracle_kind` gains `guarded_result_match` (bounded vocabulary addition,
  pinned in `policy/output_contracts.txt` and `docs/OUTPUT_SCHEMA.md`).
- The file-fact cache generation is bumped so warm pre-extension caches
  cannot reuse the old oracle semantics.

## Problem

A test that guards a helper's result with an explicit Result match —

```rust
match expect_response(&mut cursor, expected_id) {
    Ok(response) => { /* exact assertions on response fields */ }
    Err(error) => {
        if !matches!(error.downcast_ref::<ParseError>(),
                     Some(ParseError::InvalidData { .. })) {
            panic!("unexpected error variant: {error}");
        }
    }
}
```

— observes the changed owner's returned `Result` directly, and its Err arm
is the result's discriminator. No producer recognized this shape: the whole
match statement was only ever captured by accident (the mock-expectation
name sniff swallowing the block when the callee happened to start with
`expect_`), it carried no binding to the callee, and the return-value seam
kept its `observation_unverified` weakly-exposed outcome even though the
suite watches the owner's result and pins the exact error variant. This is
the #13162 `expect_response` comparison shape.

## Behavior

- A bounded guarded-Result-match scanner recognizes, inside test bodies,
  `match <scrutinee> { .. }` statements where the scrutinee is a direct
  path call `path::to::callee(args)`. Method receivers, call chains,
  macros, and a trailing `?` on the scrutinee are all rejected: the
  observed result must be the callee's own. The block must hold at least
  one `Err(` arm plus at least one other arm: an `Ok(` arm (classic
  guarded match) or a catch-all arm (the guarded-routing form
  `Err(e) if <pin> => {}, rest => <loud failure>` with no `Ok` arm — the
  exact historical `expect_response` harness shape).
- The scan is the single authority for the recognized statement: the
  generic statement joiners (mock-expectation name sniff, custom-helper
  sniff) skip its start lines in both the parser-backed and lexical
  fallback paths, so the two paths agree and the accidental whole-block
  capture is retired for this shape.
- An oracle fact (`oracle_kind: "guarded_result_match"`) is emitted only
  when every Err arm both terminates and carries a recognized
  discriminator. Terminating means a loud failure body
  (`panic!`/`assert!`/`bail!`/`return`/re-raise/unwrap), or a guarded
  accept arm (`Err(e) if <pin> => {}`): the guard must carry the pin,
  the body must be trivial, and every catch-all arm must fail loudly, so
  a guard miss routes to an observed failure; an arm that swallows the
  error, and a routing target that stays silent, never credit.
  Discriminators, strongest first:
  - an exact error-variant pin in binding position (the arm pattern
    proper, the pattern argument of a `matches!`/`assert_matches!` in
    the arm body or guard, or a guard equality `==`/`!=` against a
    variant path) ranks `strong`;
  - a concrete `.downcast[_ref|_mut]::<Type>()` pin ranks `medium`.
  Every Err arm must pin: one pinned arm beside an unpinned escape arm
  is not an exact identity. Wildcard `Err(_)` arms, opaque predicates,
  and message-only variant mentions never pin: exactness is not inferred
  from names or payload text.
- A same-named local `fn` or `let` binding in the test body defeats the
  oracle (the shared #3714 shadow authority): a shadowed name is not the
  resolved callee.
- In reveal classification, the fact confirms observation for
  `error_path` and `return_value` probes whose changed owner's bare name
  is the scrutinee callee, without any changed-line token overlap. The
  fact's text embeds the scrutinee callee, so the binding is same-entity
  by name. Effect families (side effect, call deletion) and families
  whose changed behavior need not flow through the matched result keep
  their existing observers. Wrong-owner facts stay non-confirming; their
  only association remains the pre-existing generic (weak) rules.
- Strength maps through the existing stage authority: `strong` credits
  discrimination (`exposed` when reach, infection, and propagation also
  hold); `medium` keeps the seam below `exposed` with observation
  confirmed and the typed weakness stated. The propagation and infection
  layers are unchanged and still gate the class independently.

## Non-Goals

- No credit from a bare `?` on the scrutinee, a terminal `Ok` alone, nearby
  tokens, diagnostic strings, or unresolved/custom macros.
- No recognition for variable-bound scrutinees (`let r = f(..); match r`),
  method-call scrutinees, or `Option` matches; those keep their existing
  meaning.
- No change to the propagation, infection, or reach authorities; a weak
  propagation witness still holds the finding at `weakly_exposed`.
- No per-language variants beyond Rust; no generalized semantic engine.
- No mutation-runtime outcome vocabulary.

## Required Evidence

- `fixtures/guarded_result_match_owner_observation`: the real-consumer
  reduction of the classic form (guarded Result match on the changed
  owner with exact-variant downcast Err pin and exact-field Ok
  assertions) reaching `exposed` on the producer-owned error-path seam,
  with the guarded-match oracle named in the related-test evidence.
- `fixtures/guarded_result_match_positive`: the historical #13162
  routing form (guarded accept arm with an exact error-variant equality
  routed by a loud catch-all, no `Ok` arm) plus the classic pattern-pin
  form; both probes reach `exposed` with the producer-owned oracle.
- `fixtures/guarded_result_match_fail_closed`: wrong-owner guard,
  variable-binding scrutinee, shadowed callee, message-only error
  predicate, and swallowed-error arm — all non-crediting; findings stay
  at `weakly_exposed`.
- `fixtures/guarded_result_match_swallowed`: the owner-correct harness
  whose Err arms only log or panic through a wildcard — findings stay at
  `weakly_exposed` with the missing discriminator named.
- Honesty-corpus cases pinning the positive controls
  (`rust_guarded_result_match_routing_positive_control`,
  `must_promote`) and the negative controls
  (`rust_guarded_result_match_swallowed_no_promotion`,
  `rust_guarded_result_match_fail_closed_shapes`, `must_not_promote`,
  `maximum_class weakly_exposed`).
- In-crate scanner tests: positive shapes (classic and guarded-routing),
  downcast-only medium, Err pattern pins, guard-equality pins, wildcard/
  no-op arms, silent catch-alls, unguarded trivial arms, neighbor-arm
  text isolation, multi-arm unpinned rejection, message-only diagnostics,
  non-direct scrutinees, shadow defeat, comment/string immunity.
- In-crate reveal tests: owner-bound confirmation without token overlap,
  wrong-owner non-confirmation, type-pin weakness, effect-family refusal.
- Parser-path test: exactly one guarded-match oracle for the routing
  form and no mock-expectation duplicate through
  `extract_parser_oracles`.

## Acceptance Examples

- Accept: the `expect_response` shape above credits `exposed` for an
  error-path/return-value seam of `expect_response` when reach, infection,
  and propagation hold.
- Accept: the guarded-routing form — `Err(error) if error.kind() ==
  io::ErrorKind::InvalidData => {}` routed by `result => bail!(..)` with
  no `Ok` arm — credits through the guard's equality pin.
- Accept: a downcast-only Err guard credits observation and names the type
  pin, but the seam stays below `exposed`.
- Reject: the same match naming a different callee; a `let`-bound result
  scrutinee; a local `fn expect_response` shadow; an `Err(_)` arm; an arm
  that only logs; a silent catch-all routing target; a diagnostic string
  mentioning the variant.

## Test Mapping

- `crates/ripr/src/analysis/extract/oracles/scan.rs::guarded_result_match_tests`
- `crates/ripr/src/analysis/syntax/ra.rs::guard_pipeline_debug_tests::parser_path_credits_guarded_routing_match_in_test_facts`
- `crates/ripr/src/analysis/classify/reveal.rs::tests::guarded_result_match_*`
- `fixtures/guarded_result_match_owner_observation`
- `fixtures/guarded_result_match_positive`
- `fixtures/guarded_result_match_fail_closed`
- `fixtures/guarded_result_match_swallowed`
- `fixtures/evidence-promotion-honesty-corpus/corpus.json`
  (`rust_guarded_result_match_routing_positive_control`,
  `rust_guarded_result_match_swallowed_no_promotion`,
  `rust_guarded_result_match_fail_closed_shapes`)

## Implementation Mapping

- `crates/ripr/src/analysis/extract/oracles/scan.rs` —
  `guarded_result_match_scan` and the arm/discriminator grammar.
- `crates/ripr/src/analysis/extract/calls.rs` — reused shadow authority
  (`test_body_shadows_callee`, moved from `classify/related_tests.rs`).
- `crates/ripr/src/analysis/syntax/ra.rs` — parser-path ingestion and
  statement-joiner suppression.
- `crates/ripr/src/analysis/classify/reveal.rs` — `owner_callee` context
  and the producer-owned confirmation.
- `crates/ripr/src/domain/evidence.rs` — `OracleKind::GuardedResultMatch`.
- `crates/ripr/src/analysis/seam_cache.rs` — file-fact cache generation
  bumps (1.0 -> 1.1 for the oracle kind and statement suppression;
  1.1 -> 1.2 for the guarded-routing grammar, after a warm 1.1 cache
  demonstrably replayed the pre-routing classification on a real
  fixture).

## Metrics

- unit_test_pass_rate
- golden_fixture_pass_rate
- oracle_kind histograms now count the guarded-match form under
  `guarded_result_match`.
