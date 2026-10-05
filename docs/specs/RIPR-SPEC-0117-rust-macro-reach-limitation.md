# RIPR-SPEC-0117: Rust Macro-Reach Limitation

Status: accepted

Owner: product / swarm

Created: 2026-06-18

Linked issues:

- [#1292](https://github.com/EffortlessMetrics/ripr-swarm/issues/1292)

Linked PRs:

- This PR

Support-tier impact:

- No tier change. `docs/status/SUPPORT_TIERS.md` remains unchanged. This spec
  adds one additive `static_limit_kind` value, `rust_macro_reach_unresolved`,
  and one additive `stop_reasons` value, `macro_reach_unresolved`.
- Classification stays `no_static_path`. This is a named limitation, not a
  coverage claim, test relation, repair packet, release-readiness claim, or
  macro-expansion engine.
- No `schema_version` bump is required because `static_limit_kind` and
  `stop_reasons` already exist as additive optional finding metadata.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- Register the new stop reason in `policy/output_contracts.txt`.
- Update `docs/OUTPUT_SCHEMA.md` and `.ripr/traceability.toml`.
- No new crates, dependencies, binaries, workflow permissions, process
  execution, runtime macro expansion, rustc driver integration, or LSP surface.

## Problem

RIPR-SPEC-0114 names bounded Rust helper-call chains when a test calls an entry
point that lexically reaches the changed owner. It intentionally stops at
macros. In real Rust repos, tests and public entry points often route through
`macro_rules!` helpers. Today that can leave a first-run user with bare
`no_static_path`, even though RIPR saw a plausible entry path and the unresolved
edge is specifically a macro boundary.

The honest first-run behavior is to name that boundary without pretending the
macro was expanded:

```text
no_static_path + static_limit_kind: rust_macro_reach_unresolved
```

## Behavior

### Trigger condition

After direct related-test classification returns `no_static_path` with no
related tests, and after the RIPR-SPEC-0114 bounded transitive witness check
does not find a lexical path, the Rust adapter may emit the macro-reach
limitation when all of these are true:

1. A test calls a non-owner Rust entry symbol, or directly invokes a macro.
2. A bounded BFS from that entry symbol reaches a production function that
   invokes a macro, or the test directly invokes the macro.
3. A same-repo `macro_rules!` definition with that macro name is visible in the
   indexed source.
4. That macro definition lexically mentions the changed owner name as an
   identifier.

### When found

Set `Finding.static_limit_kind = Some(StaticLimitKind::RustMacroReachUnresolved)`.
Push `StopReason::MacroReachUnresolved` to `Finding.stop_reasons`.
Push an honest limitation message and a concrete witness pointer into
`Finding.evidence`.

The witness pointer names:

- the witnessing test file and line;
- the entry symbol the test called;
- the macro invocation site;
- the macro name.

The pointer uses candidate language:

```text
The macro path may lead here. Inspect it to judge whether this change is observed.
```

### When not found

Leave the finding exactly as before. Do not emit the limitation merely because a
test file contains an unrelated macro, an external macro is invoked, or a macro
definition does not mention the changed owner name.

### Fail-closed boundaries

- Never change classification from `no_static_path`.
- Never add the witness to `related_tests`.
- Never claim the test reaches, covers, tests, or exercises the changed owner.
- Only same-repo `macro_rules!` definitions are considered.
- The macro definition scan is lexical owner-name matching only; it is not macro
  expansion, type resolution, hygiene, trait dispatch, visibility analysis, or
  rustc integration.
- If the macro definition is absent, ambiguous, generated, external, or does not
  lexically mention the owner, do not emit this limitation.
- RIPR-SPEC-0119 refines direct test-body macro witnesses to
  `rust_macro_wrapped_test_call_unresolved`; production-entry macro boundaries
  keep `rust_macro_reach_unresolved`.

## Same-File Test Generators

A test file can define a `macro_rules!` whose transcriber emits `#[test] fn`
and invoke it once per case. Those tests are real, but the parser sees only
opaque token trees, so they were never indexed and the code they call read as
unreached: `ungripped` in repo exposure and `no_static_path` in diff mode,
a false gap when the tests catch the mutant (#5334).

ripr indexes such a test when the generator fits one bounded shape, and
otherwise indexes nothing for it:

- the definition is the only `macro_rules!` of that name in the file, sits
  directly in the file or an inline module, and precedes the invocation, which
  sits at item position in the same scope or a nested one; neither the
  definition nor the invocation carries an attribute or sits under a cfg'd
  module or file;
- every arm up to the selected one has a matcher of comma-separated
  `$name:fragment` metavariables only; the selected arm is the first whose
  metavariable count matches the invocation's top-level arguments, and an
  `ident`, `literal`, `block` or `tt` argument must have that shape;
- the selected transcriber contains `#[test]`, no repetition and no call to
  another macro defined in the file, and no argument invokes one.

Each metavariable is replaced by its argument's source text and `$crate` by
`crate`; an `expr` or `literal` argument of more than one element (`-1`) is
wrapped in parentheses. Substitution is textual and ignores hygiene, so a
block or expression argument can name a binding the transcriber declares;
such a test reads as its unhygienic text does. The expansion is
parsed by the ordinary file-fact producer, and each `#[test]` function becomes
a test of the invoking file with every line pinned to the invocation.
Assertion admission (RIPR-SPEC-0197 owner pins and equality admission) runs
over the expansion text with the same rules as a hand-written test, so a
deferred or escaping assertion inside the transcriber is refused exactly as it
would be written by hand.

This is not macro expansion in general. Generators defined in another file,
`#[macro_use]` imports, repetition and procedural macros stay unindexed, and
reach through them keeps the limitations above. A generated test whose
transcriber calls a helper macro from another file is indexed for reach, but
an assertion inside that helper is refused like any untrusted macro.

## Wire Format

| Field | Value when fires | Value when not fires |
|---|---|---|
| `classification` | `no_static_path` | unchanged |
| `static_limit_kind` | `rust_macro_reach_unresolved` | omitted / unchanged |
| `stop_reasons` | includes `macro_reach_unresolved` | unchanged |
| `related_tests` | unchanged (witness is not added) | unchanged |
| `evidence` | limitation prose plus concrete macro witness pointer | unchanged |

## Non-Goals

- Full macro expansion.
- Promoting to `weakly_exposed`, `reachable_unrevealed`, or `exposed`.
- Emitting a repair packet.
- Inferring macro hygiene, generated items, trait dispatch, or visibility.
- Solving all macro-heavy Rust reachability cases.

## Acceptance Examples

1. **Macro-boundary limitation fires**: an integration test calls `outer()`;
   `outer()` invokes `call_inner!`; same-repo `macro_rules! call_inner` mentions
   changed owner `inner`. Result: `no_static_path` plus
   `static_limit_kind: rust_macro_reach_unresolved`.
2. **No owner mention**: `outer()` invokes `call_other!`, but the macro
   definition does not mention changed owner `inner`. Result: unchanged bare
   `no_static_path`.
3. **External macro**: `outer()` invokes a macro whose definition is not in the
   indexed repo source. Result: unchanged bare `no_static_path`.
4. **Lexical transitive path exists**: the RIPR-SPEC-0114 witness fires first.
   Result remains a transitive/public-API limitation, not macro-reach. For
   integration-test witnesses, RIPR-SPEC-0118 refines the kind to
   `rust_integration_public_api_path_unresolved`; non-integration witnesses
   keep `rust_transitive_reach_unresolved`.

## Required Evidence

- `StaticLimitKind::RustMacroReachUnresolved` in
  `crates/ripr/src/domain/language.rs`.
- `StopReason::MacroReachUnresolved` in `crates/ripr/src/domain/probe.rs`.
- Macro-boundary witness logic in
  `crates/ripr/src/analysis/classify/transitive_reach.rs`.
- Rust adapter wiring in `crates/ripr/src/analysis/language/rust/mod.rs` for diff
  and repo modes.
- Pure fixture: `fixtures/rust_macro_reach_limitation/`.
- Honesty corpus member in
  `fixtures/evidence-promotion-honesty-corpus/corpus.json` asserting
  `must_emit_limitation`, `expected_limit_kind: rust_macro_reach_unresolved`,
  `must_disclose_limitation_detail`, and `must_remain_non_promoted`.

## Test Mapping

- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_entry_path_stops_at_owner_macro_then_macro_witness_is_captured`
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_macro_definition_does_not_name_owner_then_macro_witness_is_none`
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_test_invokes_owner_macro_directly_then_macro_witness_is_captured`
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::macro_witness_pointer_uses_may_language_and_no_coverage_claim`
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::macro_reach_limitation_detail_names_edges_route_and_non_claim`
- `crates/ripr/src/output/human.rs::tests::human_output_surfaces_static_limitation_detail`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::expands_each_invocation_with_its_arguments_and_lines`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::repetition_shadowing_scope_and_order_refuse`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::arm_selection_follows_count_and_fragment_shape`
- `crates/ripr/src/analysis/facts/macro_generated_tests.rs::tests::macro_generated_tests_are_indexed_at_their_invocations`
- `crates/ripr/tests/macro_generated_tests.rs::a_macro_generated_test_reads_like_the_hand_written_test`
- `crates/ripr/tests/macro_generated_tests.rs::refused_shapes_and_uninvoked_generators_add_no_test`
- `crates/ripr/tests/macro_generated_tests.rs::a_deferred_assertion_in_a_generated_test_is_refused_like_a_hand_written_one`
- `fixtures/rust_macro_reach_limitation/expected/check.json`
- `cargo xtask check-evidence-promotion-honesty`

## Implementation Mapping

| Component | Location |
|---|---|
| Static limit enum | `crates/ripr/src/domain/language.rs` |
| Stop reason enum | `crates/ripr/src/domain/probe.rs` |
| Macro witness producer | `crates/ripr/src/analysis/classify/transitive_reach.rs` |
| Same-file test generator expansion | `crates/ripr/src/analysis/syntax/local_test_macros.rs` |
| Generated test indexing | `crates/ripr/src/analysis/facts/macro_generated_tests.rs` |
| Generated test assertion admission | `crates/ripr/src/analysis/classify/owner_pin.rs` |
| Classifier export | `crates/ripr/src/analysis/classify/mod.rs` |
| Diff/repo-mode wiring | `crates/ripr/src/analysis/language/rust/mod.rs` |
| Human witness and limitation-detail projection | `crates/ripr/src/output/human/sections.rs` |
| Output contract docs | `docs/OUTPUT_SCHEMA.md` |
| Pure fixture | `fixtures/rust_macro_reach_limitation/` |
| Honesty corpus case | `fixtures/evidence-promotion-honesty-corpus/corpus.json` |

## CI Proof

- `cargo test -p ripr analysis::classify::transitive_reach`
- `cargo test -p ripr analysis::language::rust`
- `cargo xtask fixtures rust_macro_reach_limitation`
- `cargo xtask check-evidence-promotion-honesty`
- `cargo xtask check-output-contracts`
- `cargo xtask check-static-language`
- `cargo xtask check-spec-format`
- `cargo xtask check-spec-numbering`
- `cargo xtask check-traceability`
- `cargo fmt --check`

## Metrics

- Golden fixture pass: `fixtures/rust_macro_reach_limitation`.
- Corpus invariant: `rust_macro_reach_named_limitation` emits
  `rust_macro_reach_unresolved`, discloses a witness and limitation detail, and remains
  non-promoted at `no_static_path`.
- Existing transitive-reach fixture behavior remains unchanged.
