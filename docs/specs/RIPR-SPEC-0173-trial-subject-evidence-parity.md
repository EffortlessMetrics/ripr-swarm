# RIPR-SPEC-0173: harness trial subject evidence parity

Status: proposed

Owner:

Created: 2026-09-03

Linked proposal:

Linked ADRs:

Linked plan:

Linked issues:

- #3603 (finding: trial subject evidence parity — helper-callback bodies
  and method unwrap/expect oracles)
- #3532 (the harness registry and libtest_mimic adapter this parity
  contract binds)

Linked PRs:

Support-tier impact:

- No tier change. The parity contract widens which parsed evidence a
  registered trial subject claims inside the existing static analysis;
  it adds no runtime evidence, no harness execution, and no new
  analysis surface. [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)
  remains the tier authority.

Policy impact:

- The classified seam-cache generations bump with any semantic change to
  trial subject evidence (CACHE_SCHEMA_VERSION and its sharded/compact
  twins), so pre-change caches cannot serve evidence-blind
  classifications.

## Problem

A registered libtest_mimic trial subject is one named invocation, but
what the trial exercises is not the invocation text — it is the body of
the callback the invocation hands to the harness, plus whatever the
callback body asserts. The first adapter generation scanned only the
invocation argument span, so two under-emit gaps opened relative to an
ordinary `#[test]` that reaches the same behavior:

1. `Trial::test("name", helper_fn)` recorded no call, oracle, or literal
   evidence from `helper_fn`'s body — production behavior reached only
   through the callback stayed unobserved by that subject.
2. Ordinary `#[test]` parsing records `.unwrap()`/`.expect()` method
   calls as smoke evidence; the trial token scanner recorded only
   assertion macros, so trial subjects understated observation evidence
   relative to equivalent ordinary tests.

Both directions under-emit (fail closed), and the fix must not open the
opposite failure: crediting a same-named function the callback does not
actually name is the token-coincidence false-`exposed` family.

## Behavior

A named-invocation subject keeps its identity span and body — the
registration invocation — while its `calls`, `assertions`, and
`literals` widen over exactly the code the subject exercises. Two
widening directions, each fail-closed:

- a bare-identifier callback (`Trial::test("name", helper_fn)`) that
  resolves to exactly one function in the registered target contributes
  that function's parsed body evidence one level deep, with real line
  attribution. Resolution is admitted only when binding identity is
  provable: no local binding of the name inside the trial's enclosing
  body (`let`, parameter, closure or loop pattern, or a fn-local `use`),
  no top-level `use` binding the same leaf name, and exactly one
  same-named function that is a top-level `fn` item of the file. A fn
  that lives only inside a nested module or impl is not name-visible to
  the invocation and never admits. Transitive callee bodies stay
  unclaimed — one level is the contract;
- method-position `.unwrap()`/`.expect()` calls inside the claimed span
  register smoke oracles (`SmokeOnly`/`Smoke`) with the receiver
  expression as text and real source line attribution — the same
  evidence the ordinary `#[test]` parser records for
  `ast::MethodCallExpr`. Keyword receiver participants (`self`, `Self`,
  `await`, `crate`, `super`) are ordinary postfix receivers, so
  `self.value().unwrap()` never truncates to `value.unwrap()`. The
  receiver walk crosses only balanced bracket groups, generic argument
  groups, literals, identifiers, those keywords, and `.`/`::`
  connectors, and never reaches across a statement boundary.

Macro input is skipped wholesale for method scanning (assertion macros
still classify themselves), matching what parsed method-call nodes could
see on the ordinary path: a method token inside `println!(...)` never
classifies. Qualified assertion invocations slice from the full
contiguous macro path (`insta::assert_snapshot![...]`, including `crate`,
`self`, and `super` segments), byte-for-byte like the ordinary parser's
macro-call slice. A dormant `macro_rules!` definition inside the claimed
span or inside a helper body is a template that never executes: nothing
inside it — assertion macro, method call, call fact, or literal — joins
the subject's evidence, at the token level (a definition inside another
macro's token tree carries no node to query) and at the file level (the
same ancestor authority that keeps dormant templates from becoming
subjects), and the template's source span is erased from every merged
lexical extraction so live same-line evidence survives. Lexical call and
literal extraction masks comments (line and nested block) and string
contents before scanning, so commented-out code is never evidence.
Closures, path callbacks, unresolved names, ambiguous names, and
shadowed names contribute nothing beyond the invocation span.

## Non-Goals

- No assertion-form parity beyond trial subjects: #3284 owns
  assertion-form parity generally, and this spec only binds the
  libtest_mimic trial scanner to the same method-oracle evidence the
  ordinary parser records. RIPR-SPEC-0153 keeps assertion-form parity
  out of its contract, and this spec does not restore it there.
- No transitive helper bodies: one level deep is the boundary; a
  callee-of-the-callback stays unclaimed.
- No name-heuristic crediting: resolution consumes producer-owned
  function facts and parsed structure only, and every ambiguity fails
  closed.
- No strength promotion: helper-body evidence reuses the ordinary
  parser path, and method oracles are smoke-strength by construction;
  no oracle may claim a strength the underlying evidence cannot
  support.
- No mutation execution, no coverage claims, and no vocabulary beyond
  the static finding contract.

## Required Evidence

- a discriminating positive: an unshadowed top-level helper callback's
  call, macro-oracle, smoke-oracle, and literal evidence joins the
  subject with real source lines, and the mirrored `TestFact` carries
  the same evidence;
- negative controls proving each fail-closed boundary: a local `let`
  closure shadowing a same-named file-level fn, a same-named fn
  parameter, a fn that exists only inside a nested module, and a
  top-level `use` binding the callback's leaf name — none of the
  same-named fn's body evidence may be claimed while the subject still
  classifies;
- a one-level boundary pin: the transitive callee's body stays
  unclaimed;
- a discriminating method-oracle positive: `.unwrap()` and `.expect()`
  inside a trial body register `SmokeOnly`/`Smoke` oracles with
  receiver-ful text and exact source lines, while the assertion-macro
  oracle on the same subject carries its real source line;
- keyword-receiver positives: `self.value().unwrap()`,
  `self.value().await.unwrap()`, and a chained form carry the full
  receiver text, exact lines, and observed receiver tokens;
- fail-closed method-oracle controls: a struct field named `expect`, a
  path-shaped `Result::unwrap(...)` call, and method tokens inside
  non-assertion macro input never classify;
- dormant-template controls: a `macro_rules!` definition inside a trial
  closure and inside a helper body contributes no oracle, call, or
  literal from the template (the full expected evidence set, exactly)
  while the subject still classifies and live surrounding helper
  evidence still admits; the template spans and masking are pinned as a
  mechanism, and live calls, literals, and assertions sharing the
  template's line survive;
- comment-masking controls: block-commented calls and numbers inside a
  helper body contribute no evidence while live surrounding evidence
  still admits;
- qualified-path parity: `insta::assert_snapshot![...]` and
  `crate::snap::assert_json_snapshot!(...)` oracle texts carry the full
  macro path and match the ordinary parser's slice;
- a warm-cache regression: a classified-seam envelope seeded under the
  previous generation must miss the current generation's key with
  identical identity fields, and the schema-generation pin moves in the
  same PR as the semantic change.

## Acceptance Examples

- `Trial::test("beta", check_beta)` with `fn check_beta()` calling
  `parse_config` and asserting: the `beta` subject (and its `TestFact`)
  observes `parse_config` with the helper's real line and claims the
  helper's assertion and smoke oracles.
- The same registration with `let check_beta = || ...;` above it: the
  subject classifies but claims none of the file-level `check_beta`'s
  body.
- A trial body calling `parse_port("8080").unwrap()`: the subject gains
  a smoke oracle whose text is the full receiver chain, on the call's
  real line — the same evidence an ordinary `#[test]` with the same
  body would carry.
- `self.value().await.unwrap()` inside a trial: the oracle text is the
  full chain, not `value().await.unwrap()`.
- A registered harness workspace analyzed on a pre-change cache: the
  generation bump forces a miss, so the widened evidence is recomputed
  rather than served stale.

## Test Mapping

- `crates/ripr/src/analysis/facts/harness_registry/tests.rs::helper_callback_bodies_contribute_one_level_of_subject_evidence`
- `crates/ripr/src/analysis/facts/harness_registry/tests.rs::trial_method_unwrap_expect_oracles_carry_real_lines_and_receivers`
- `crates/ripr/src/analysis/facts/harness_registry/tests.rs::shadowed_callback_names_fail_closed`
- `crates/ripr/src/analysis/facts/harness_registry/tests.rs::trial_method_oracle_receivers_carry_keyword_and_chained_forms`
- `crates/ripr/src/analysis/facts/harness_registry/tests.rs::given_dormant_macro_rules_template_when_trial_scanned_then_no_smoke_oracle_from_template`
- `crates/ripr/src/analysis/facts/harness_registry/tests.rs::given_dormant_macro_rules_template_in_helper_callback_then_no_oracle_either`
- `crates/ripr/src/analysis/facts/harness_registry/tests.rs::dormant_template_spans_cover_the_parsed_definition_and_mask_exactly`
- `crates/ripr/src/analysis/facts/harness_registry/tests.rs::given_block_commented_helper_evidence_then_calls_and_literals_stay_inert`
- `crates/ripr/src/analysis/facts/harness_registry/tests.rs::given_one_line_dormant_macro_then_live_same_line_evidence_survives`
- `crates/ripr/src/analysis/facts/harness_registry/tests.rs::trial_qualified_assertions_keep_the_full_path`
- `crates/ripr/src/analysis/seam_cache.rs::tests::previous_generation_classified_seam_envelope_with_identical_identity_is_a_miss`
- `crates/ripr/src/analysis/seam_cache.rs::tests::schema_versions_pin_the_role_composition_generation`

## Implementation Mapping

- `crates/ripr/src/analysis/facts/harness_registry.rs` —
  `apply_libtest_mimic_target` (evidence merge),
  `bare_ident_callback`, `resolve_helper_function`,
  `enclosing_function`, `enclosing_body_binds_name`,
  `parser_oracles_for_node_tokens` (method oracles, macro-input skip),
  `receiver_start_index` (keyword receiver participants)
- `crates/ripr/src/analysis/facts/model.rs` — the evidence-boundary
  documentation on `HarnessSubjectFact` and
  `HarnessSubjectClaim::NamedInvocation`
- `crates/ripr/src/analysis/seam_cache.rs` — classified/sharded/compact
  schema-generation bumps and the warm-cache regression

## Metrics

- unit_test_pass_rate
- golden_fixture_pass_rate
