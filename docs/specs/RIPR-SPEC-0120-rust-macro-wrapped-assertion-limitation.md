# RIPR-SPEC-0120: Rust Macro-Wrapped Assertion Limitation

Status: accepted

Owner: product / swarm

Created: 2026-06-20

Linked issues:

- First-run-trust / P3 Rust macro limitation taxonomy: name custom
  assertion-like macro observers separately from reach-blocking macro calls.

Linked PRs:

- This PR

Support-tier impact:

- None for support-tier status. This spec adds one additive
  `static_limit_kind` value, `rust_macro_wrapped_assertion_unresolved`.
- Classification stays reachable-but-undiscriminated. The value names an
  unresolved assertion macro, not a reach claim, oracle claim, repair packet, or
  macro expansion proof.
- No `schema_version` bump is required because `static_limit_kind` is already
  additive optional finding metadata.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- Update `docs/OUTPUT_SCHEMA.md`, `docs/STATIC_LIMITS.md`, and
  `.ripr/traceability.toml`.
- No new crates, dependencies, workflow permissions, runtime executors, macro
  expansion, rustc driver integration, or LSP-specific behavior.

## Problem

RIPR can already recognize direct Rust owner calls in tests. A real repository
may then observe the result through a custom assertion macro such as
`assert_result!(actual, expected)`. RIPR does not expand or classify that macro,
so a plain `reachable_unrevealed` result hides the most useful first-run fact:
there is a related test, but the discriminator is behind an unsupported macro
observer.

## Behavior

After normal Rust classification, if a finding:

- is `reachable_unrevealed`;
- has related Rust tests;
- has no selected recognized oracle; and
- at least one related test body invokes an assertion-like macro that is not in
  RIPR's known assertion macro set,

then set:

```text
static_limit_kind = rust_macro_wrapped_assertion_unresolved
```

The finding also emits witness and structured limitation-detail evidence using
the shared prefixes for:

- last established edge;
- first unresolved edge;
- analyzer route;
- non-claim.

The analyzer route is:

```text
analysis/rust-macro-assertion-oracle
```

### Weakly exposed by a proximity oracle (#7089)

A finding can read `weakly_exposed` only because a test that merely shares the
owner's file (`same_test_file`, `same_module` or a name-token relation) holds a
weak recognized oracle, while every test that calls or reaches the owner has
none. The same limit applies to that finding when all of these hold:

- the finding is `weakly_exposed` and has no static limit yet;
- the related-test window is complete (no matched test was truncated away);
- at least one related test has a direct relation (not proximity and not
  name-only: `same_test_file`, `same_module`, `weak_token_substring` and
  `owner_named_test` do not count), and no such test, nor any test without a
  relation reason, has a recognized oracle or an assertion ripr refused to
  credit (`assertion_not_credited`);
- at least one proximity or name-only test does hold a recognized oracle;
- one of the direct tests invokes an assertion-like macro outside the known
  set, and the macro's delimited arguments make a bare call `owner(`, read
  with comments and strings masked. A path- or method-qualified call
  (`Bar::new(`, `x.len(`) may name another item with the same last segment
  and does not count, nor does an argument list that runs past a `;` or holds
  a `'` (char literals are not masked and can unbalance the delimiters) or a
  nested macro invocation (`stringify!(owner(1))` never evaluates its tokens);
- the parser established the owner as a free function. For a method owner
  (`impl Cart::total`) or a trait default method a bare `total(` can only be
  another item, so the path does not run; the lexical fallback leaves the
  context unknown, so it does not run there either.

The class stays `weakly_exposed`; the finding names the limit instead of
reporting a gap. An owner value that reaches the macro only through a binding,
or a direct test that also holds a recognized oracle, keeps the gap.

### Same-file macro whose arms show a recognized assertion (#9122)

A macro defined in the test's own file is not unresolved when every arm of its
unique, un-gated, in-scope `macro_rules!` definition itself contains an
assertion the ordinary extractor recognizes (`assert_eq!`, `assert!`, ...) over
a `$fragment` that arm's matcher binds, and the test calls it as a plain
statement of its own body. A tautology (`assert!(true)`, `assert_eq!(1, 1)`,
`assert_eq!($x, $x)`), a self-recursive macro and a call whose argument count
no arm takes credit nothing. Only the asserted operands count (the first for
`assert!`, the first two for `assert_eq!`/`assert_ne!`); a fragment that
appears only in a diagnostic message does not attribute the assertion. A
transcriber with a branch, loop, `match`, closure, early exit or repetition
credits nothing, because its assertion may not run. An unrelated assertion on
the same line as the call does not suppress the credit. The call then earns one oracle of kind `unknown`
and strength `unknown`, never `weak` or stronger, so it records that an
assertion exists without grading what it pins. A hand-rolled
`if bad { panic!() }` body, a macro that only logs, a macro with one arm that
shows no assertion, a duplicate, imported or cfg-gated definition, a macro
from another file or crate, and a call in a closure, branch or `use`-bearing
test keep this limit.

### Helper call with a macro among its arguments (#9122)

An `assert_*` helper call (`common::assert_matches(snapbox::file![..], cmd)`)
whose arguments contain a macro is still recognized as a helper call when its
callee, the text before the first `(`, is the `assert_*` path. The macro hides
what the call compares, so the call is graded `unknown` and never exact, even
for an equality-named helper. A macro invocation (`assert_eq!(..)`) or a
macro in the callee position is not a helper call. Negation and inequality
arguments (`!x`, `a != b`) do not change this grade.

## Fail-Closed Boundaries

- Never promote the finding because a custom assertion macro exists.
- Never infer assertion semantics from a generic `assert_*!(...)` name.
- Never emit a repair packet or receipt command for this limitation.
- Never claim the macro expands, reaches, covers, tests, exercises, or
  discriminates the changed behavior.
- Known assertion macros such as `assert_eq!` remain handled by the existing
  oracle extractor, not by this limitation.

## Wire Format

| Field | Value |
|---|---|
| `classification` | `reachable_unrevealed`, or `weakly_exposed` under the proximity-oracle rule (#7089) |
| `static_limit_kind` | `rust_macro_wrapped_assertion_unresolved` |
| `related_tests` | preserved from direct reach classification |
| `static_limitation.analyzer_route` | `analysis/rust-macro-assertion-oracle` |

## Acceptance Examples

1. **Custom assertion macro**: `tests/it.rs` directly calls changed owner
   `inner`, then invokes `assert_result!(result, 7)`. RIPR keeps the finding
   `reachable_unrevealed`, emits
   `static_limit_kind: rust_macro_wrapped_assertion_unresolved`, surfaces the
   assertion macro under "Where to look" and "Limitation detail", and emits no
   repair packet.

   Fixture: `fixtures/rust_macro_wrapped_assertion_limitation/`.

2. **Known assertion macro**: `assert_eq!(result, 7)` remains handled by the
   existing Rust assertion extractor. The new limitation does not fire for
   built-in assertion macros.
3. **Proximity oracle beside an unread macro** (#7089): the only test calling
   changed owner `bulk` runs `assert_flag!(bulk(100), true)`; another test in
   the file asserts `assert!(needs_review(5000))`. RIPR keeps the finding
   `weakly_exposed` and emits
   `static_limit_kind: rust_macro_wrapped_assertion_unresolved`. With
   `let flag = bulk(100); assert_flag!(flag, true);` the gap stays.
   Corpus: `checkout-bulk-custom-assert-macro`. `strsim-sorensen-dice-equal`
   keeps its gap: its related-test window is truncated, so a hidden direct
   test's oracle cannot be ruled out.

## Required Evidence

- `StaticLimitKind::RustMacroWrappedAssertionUnresolved` in
  `crates/ripr/src/domain/language.rs`.
- Rust adapter post-classification selection in
  `crates/ripr/src/analysis/language/rust/oracles.rs`.
- Unit guards for custom assertion macros and known assertion macro
  non-matches.
- Pure fixture golden for
  `fixtures/rust_macro_wrapped_assertion_limitation/`.
- Honesty corpus case expecting
  `rust_macro_wrapped_assertion_unresolved`, limitation detail,
  non-promotion, no verify command, no receipt command, and no repair packet.
- Output schema, static-limit documentation, spec index, doc-artifact ledger,
  LSP gap-artifact validation, and traceability entries updated.

## Test Mapping

- `crates/ripr/src/analysis/language/rust/oracles.rs::tests::macro_wrapped_assertion_limit_names_reachable_unobserved_assertion_macro`
- `crates/ripr/src/analysis/language/rust/oracles.rs::tests::macro_wrapped_assertion_limit_ignores_known_assertion_macros`
- `crates/ripr/src/analysis/language/rust/oracles.rs::tests::owner_scoped_assertion_scan_requires_an_owner_call_in_the_arguments`
- `crates/ripr/src/analysis/language/rust/oracles.rs::tests::weakly_exposed_limit_needs_a_complete_direct_window_and_a_proximity_oracle`
- `crates/ripr/src/analysis/language/rust/proximity_reach_tests.rs::an_owner_call_inside_an_unread_assertion_macro_names_the_macro_limit`
- `crates/ripr/src/analysis/language/rust/proximity_reach_tests.rs::an_unread_assertion_macro_without_the_owner_call_keeps_the_gap`
- `crates/ripr/src/analysis/language/rust/proximity_reach_tests.rs::a_direct_test_with_a_recognized_oracle_keeps_the_gap`
- `crates/ripr/src/analysis/language/rust/proximity_reach_tests.rs::an_owner_call_quoted_by_stringify_keeps_the_gap`
- `crates/ripr/src/analysis/language/rust/proximity_reach_tests.rs::a_trait_default_method_owner_keeps_the_gap`
- `crates/ripr/src/domain/language.rs::tests::static_limit_kind_wire_strings_are_stable`
- `crates/ripr/src/domain/language.rs::tests::static_limit_kind_describe_is_present_and_distinct`
- `crates/ripr/src/lsp/gap_artifacts.rs::tests::validation_accepts_rust_macro_wrapped_assertion_static_limit_kind`
- `fixtures/rust_macro_wrapped_assertion_limitation/expected/check.json`
- `fixtures/evidence-promotion-honesty-corpus/corpus.json` case
  `rust_macro_wrapped_assertion_named_limitation`
- `cargo xtask check-evidence-promotion-honesty`

## Implementation Mapping

| Component | Location |
|---|---|
| Static limit enum and text | `crates/ripr/src/domain/language.rs` |
| Rust post-classification selection | `crates/ripr/src/analysis/language/rust/oracles.rs` |
| LSP gap-artifact known-kind validation | `crates/ripr/src/lsp/gap_artifacts.rs` |
| Output contract docs | `docs/OUTPUT_SCHEMA.md` |
| Static limit docs | `docs/STATIC_LIMITS.md` |
| Pure fixture golden | `fixtures/rust_macro_wrapped_assertion_limitation/` |
| Honesty corpus | `fixtures/evidence-promotion-honesty-corpus/corpus.json` |

## CI Proof

- `cargo test -p ripr macro_wrapped_assertion_limit`
- `cargo test -p ripr static_limit_kind`
- `cargo test -p ripr validation_accepts_rust_macro_wrapped_assertion_static_limit_kind`
- `cargo xtask fixtures rust_macro_wrapped_assertion_limitation`
- `cargo xtask check-evidence-promotion-honesty`
- `cargo xtask goldens check`
- `cargo xtask check-output-contracts`
- `cargo xtask check-static-language`
- `cargo xtask check-spec-format`
- `cargo xtask check-spec-numbering`
- `cargo xtask check-traceability`
- `cargo fmt --check`

## Metrics

- Corpus invariant: custom assertion-macro observers emit
  `rust_macro_wrapped_assertion_unresolved`, disclose witness and limitation
  detail, and remain non-promoted at `reachable_unrevealed`.
- Known Rust assertion macros continue through the existing oracle extractor,
  not this limitation.

## Non-Goals

- General macro expansion.
- Macro-aware promotion.
- Custom assertion macro body inspection.
- Type-directed assertion helper resolution.
- Public API reach improvements.
- External pinned-corpus execution.
