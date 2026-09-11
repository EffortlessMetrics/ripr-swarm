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
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

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
  discriminator. Terminating means, under a bounded depth-0 statement
  grammar (the masked arm body splits into top-level statements), that at
  least one depth-0 statement is unconditionally diverging — a statement
  whose whole form is a `panic!`/`unreachable!`/`unimplemented!`/`todo!`/
  `bail!` invocation, a `return ..` statement, a `process::exit(..)`
  statement, or the body-predicate failure form (a depth-0 `if` whose
  condition carries the negated changed-error pin — a `!matches!` variant
  pattern, a `!=`-against-variant equality, or a
  `.downcast::<T>()..is_none()` test — and whose block diverges) — or a
  guarded accept arm (`Err(e) if <pin> => {}`): the guard must carry the
  pin, the body must be trivial, and every catch-all arm must fail
  loudly, so a guard miss routes to an observed failure. Markers merely
  nested inside `if`/`match`/closure blocks, `.unwrap()`/`.expect()`
  statements, `assert!` forms, and conditional failures never terminate:
  an arm that can return normally swallows the error, and crediting it
  would fabricate a discriminator. Residual (documented under-credit): a
  bare `if error != Type::Variant { panic!(..) }` body terminates but
  does not pin — the pin authority does not read body inequalities as
  exact-variant identity. An arm that swallows the error, and a routing
  target that stays silent, never credit.
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
  oracle for a BARE one-segment scrutinee (the shared #3714 shadow
  authority): a shadowed name is not the resolved callee. A qualified
  scrutinee (`helpers::parse`) cannot be shadowed by a local binding, so
  the defeat does not apply to it; its owner confirmation stays
  unverified downstream (#3727 tracks qualified-path identity
  resolution).
- In reveal classification, the fact confirms observation for
  `error_path` and `return_value` probes whose changed owner's bare name
  is the scrutinee callee, without any changed-line token overlap, under
  two fail-closed gates (#3731 review): the oracle text must embed a
  BARE one-segment scrutinee (`match <owner>(..)` — a qualified path's
  identity is unresolvable at name level, so a qualified same-named
  callee never confirms), and when the changed expression constructs an
  exact error variant, the guarded pin must name that exact variant — a
  sibling-variant or type-only guard leaves the observation
  `observation_unverified`, and the shared enum-qualifier token is not a
  specificity signal. The fact's text embeds the scrutinee callee, so
  the binding is same-entity by name. Effect families (side effect, call
  deletion) and families whose changed behavior need not flow through
  the matched result keep their existing observers. Wrong-owner facts
  stay non-confirming; their only association remains the pre-existing
  generic (weak) rules. In repo-mode grading, the guarded oracle
  kind-matches error and return-value seams and
  `oracle_discriminates_seam` applies the same exact-variant rule: a
  pin that names a sibling variant or no variant does not discriminate.
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
- `fixtures/guarded_result_match_sibling_variant`: the owner's changed
  line constructs `Err(ParseError::InvalidData)` while the only guarded
  match pins the sibling `Err(ParseError::UnexpectedEof)` on a bare
  scrutinee — findings stay at `weakly_exposed` with the observation
  unverified (never `exposed`).
- `fixtures/guarded_result_match_conditional_failure`: guarded matches
  whose pinned Err arms can return normally (conditional panic,
  unrelated unwrap, closure-nested panic) — no oracle is emitted and
  findings stay at `no_static_path`.
- Honesty-corpus cases pinning the positive controls
  (`rust_guarded_result_match_routing_positive_control`,
  `must_promote`) and the negative controls
  (`rust_guarded_result_match_swallowed_no_promotion`,
  `rust_guarded_result_match_fail_closed_shapes`,
  `rust_guarded_result_match_sibling_variant_no_promotion`,
  `rust_guarded_result_match_conditional_failure_no_promotion`,
  `must_not_promote`, `maximum_class weakly_exposed`).
- In-crate scanner tests: positive shapes (classic and guarded-routing),
  downcast-only medium, Err pattern pins, guard-equality pins, wildcard/
  no-op arms, silent catch-alls, unguarded trivial arms, neighbor-arm
  text isolation, multi-arm unpinned rejection, message-only diagnostics,
  non-direct scrutinees, shadow defeat, comment/string immunity,
  conditional-panic / conditional-return / unrelated-unwrap /
  closure-nested non-termination, negated-pin condition termination,
  first-comma `matches!` pattern slices, bare-only shadow scoping.
- In-crate reveal tests: owner-bound confirmation without token overlap,
  wrong-owner non-confirmation, type-pin weakness, effect-family refusal,
  sibling-variant non-confirmation with its exact-variant positive
  control, qualified-scrutinee non-confirmation.
- Repo-grading tests: guarded-match kind matching for error and
  return-value seams, exact-pin discrimination with sibling/type-only
  rejection, exemplar nomination for the new kind.
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
  mentioning the variant; a guarded match pinning a sibling variant of
  the changed error; a qualified scrutinee sharing the owner's bare name;
  an Err arm whose panic fires only under an unrelated condition; an Err
  arm whose only failure action is an unrelated `.unwrap()`.

## Test Mapping

- `crates/ripr/src/analysis/extract/oracles/scan.rs::guarded_result_match_tests`
- `crates/ripr/src/analysis/syntax/ra.rs::guard_pipeline_debug_tests::parser_path_credits_guarded_routing_match_in_test_facts`
- `crates/ripr/src/analysis/classify/reveal.rs::tests::guarded_result_match_*`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::guarded_result_match_*`
- `crates/ripr/src/output/agent_seam_packets.rs::tests::kind_gate_error_variant_seam_with_guarded_result_match_strong_test_is_nominated`
- `fixtures/guarded_result_match_owner_observation`
- `fixtures/guarded_result_match_positive`
- `fixtures/guarded_result_match_fail_closed`
- `fixtures/guarded_result_match_swallowed`
- `fixtures/guarded_result_match_sibling_variant`
- `fixtures/guarded_result_match_conditional_failure`
- `fixtures/evidence-promotion-honesty-corpus/corpus.json`
  (`rust_guarded_result_match_routing_positive_control`,
  `rust_guarded_result_match_swallowed_no_promotion`,
  `rust_guarded_result_match_fail_closed_shapes`,
  `rust_guarded_result_match_sibling_variant_no_promotion`,
  `rust_guarded_result_match_conditional_failure_no_promotion`)

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
- `crates/ripr/src/analysis/seam_cache.rs` — cache generation bumps:
  file-fact 1.0 -> 1.1 for the oracle kind and statement suppression;
  1.1 -> 1.2 for the guarded-routing grammar, after a warm 1.1 cache
  demonstrably replayed the pre-routing classification on a real
  fixture; 1.2 -> 1.3 for the #3731 review grammar fixes (bounded
  depth-0 terminal arms, first-comma `matches!` slices, bare-only shadow
  scoping); classified `CACHE_SCHEMA_VERSION` 1.6 -> 1.7, sharded
  0.12 -> 0.13, and compact 0.13 -> 0.14 so warm classified envelopes
  derived from pre-fix oracle facts cannot serve stale discrimination.

## Metrics

- unit_test_pass_rate
- golden_fixture_pass_rate
- oracle_kind histograms now count the guarded-match form under
  `guarded_result_match`.
