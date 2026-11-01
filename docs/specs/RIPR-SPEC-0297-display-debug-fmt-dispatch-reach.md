# RIPR-SPEC-0297: Display and Debug fmt dispatch reach

Status: proposed

Owner: product / analysis

Created: 2026-10-21

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8999 (dogfood: test asserting the exact Debug string is not linked to the
  changed `impl Debug::fmt`)

Linked PRs:

- #9026

Support-tier impact:

- No tier change. This adds one admission route to the existing
  `direct_owner_call` relation. It adds no oracle kind, class, relation
  reason or limitation kind, and no actionability widening.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml` and
  `policy/doc-artifacts.toml`.
- No schema version bump and no new wire shape.

## Problem

A changed `impl Display for T` / `impl Debug for T` method `fmt` is never
named by its tests: `format!("{:?}", err)`, `err.to_string()` and
`write!(out, "{}", err)` run it through the formatting machinery. Name-based
call matching therefore reported "no test is seen calling fmt" even when a
test asserted the exact formatted string.

## Behavior

A test is related to a changed `fmt` as `direct_owner_call` only when every
link is established from syntax:

1. The owner is `fmt` of a `Display` or `Debug` impl; generic arguments of
   the self type are ignored.
2. Exactly one indexed `fmt` carries that trait and self type, and exactly
   one `struct`/`enum`/`union`/`type` of that name is declared in the index
   (a same-named type elsewhere, for example with a derived `Debug`, refuses).
3. A formatting macro (`format`, `format_args`, `print(ln)`, `eprint(ln)`,
   `write(ln)`), not rebound by `macro_rules!` or a non-std `use`, has a
   placeholder whose trait matches the impl (`{}` for `Display`, `{:?}` for
   `Debug`), or `.to_string()` is called for `Display` when no indexed
   `to_string` belongs to an impl of the type.
4. The placeholders account for every positional argument and every named
   argument exactly; width/precision arguments (`$`, `*`) refuse.
5. The argument is built as the type by a struct literal, tuple struct, enum
   variant, the bare type name, or a `let` initialized that way or annotated
   with the type. Associated consts and factory functions are not typed,
   except a path call `Type::name(..)` (bare or `crate`/`self`/`super`
   prefix, nothing chained) to an inherent associated fn the index declares
   once, without `self`, synchronously, returning exactly `Self` or the type.
   Path prefixes before the type must be lowercase module segments, so a
   variant named like the type (`Error::DecodeError(..)`,
   `crate::Error::DecodeError(..)`) is not the type.
6. A named operand is typed only when its identifier appears in the test
   body solely at the single typed `let [mut] name` and as an argument of a
   formatting macro or the receiver of `.to_string()`. Any other whole-word
   occurrence (a pattern, closure or `fn` parameter, struct shorthand, match
   arm, another expression or call) leaves it untyped. This is a closed list
   of allowed shapes, not an enumeration of binders. Macro arguments count
   only when plain: arguments containing a block, closure, `match`, `if`,
   `let` or `=>` never vouch for an occurrence, so a binder inside an
   enclosing macro's block is refused (a nested macro is judged on its own
   arguments).

Existing shadow checks for the owner's type and trait still apply. Derived
`Debug` has no indexed `fmt` owner and is out of scope.

### Pinned Display output (#9608)

Reach alone leaves a changed `Display::fmt` that only calls
`write!(f, "literal", args)` at `weakly_exposed`: the `write!` is not a
returned value, and the test never names `fmt`. Three links close that read, and
all are required:

- Propagation. When the owner is a `fmt` whose whole body is one
  `write!` into its own formatter parameter, with a string-literal format and
  arguments free of macros, closures, blocks, branches, `?` and `.await`, and no
  `format!` or `write!` binding in the file, the same direct-output witness as
  a `format!` tail completes (`fmt_write_owner_tail`).
- Observation. An exact-value `assert_eq!` whose one side is a string literal
  and whose other side is `value.to_string()`, with `value` typed as the owner's
  self type by the strict rule above, pins the whole text the owner writes. It is
  admitted only for `Display` and only when no inherent `to_string` competes.
  `use std::fmt;` does not defeat the pin, because a test never calls `fmt` by
  that bare name.

- Stimulus (#9836). The original lexical literal-edit boundary stays the
  withholding ceiling for placeholder, argument and cancelling-fragment edits;
  the syntax-backed reader can also admit a fixed-fragment edit the lexical
  scanner mis-parsed (a quote-bearing character-literal argument), which keeps
  its credit because an exact whole-output pin observes a real fixed-fragment
  change. Ordinary string tokens must validate and decode. The template
  retains identical ordered placeholder groups after Unicode/ASCII decoding;
  doubled braces form fixed output; a brace may be the fill character before an
  alignment marker. Groups and fixed fragments compare by their written source.
  Exactly one fixed fragment differs and all other source tokens, including
  arguments and placeholder groups, stay unchanged; re-spelling fixed text prints
  the same output and keeps the pin (corpus row `idiom-display-string-pin`),
  while re-spelling a placeholder is a group edit and withholds.
  Cancelling edits across multiple fragments, argument-string edits
  that precision can hide, malformed literals/groups and raw strings withhold
  credit. Nested argument replacements in a width or precision (`{:{}}`,
  `{:.{}}`) are valid format syntax this reader does not interpret: they
  withhold credit as a named conservative gap, not as malformed input.
  This is withhold-only and does not interpret the full format-specifier
  grammar. Public API controls and the real-producer honesty fixture discriminate
  the original placeholder false-credit case.

A `format!` or `{}` side, a computed expected string and a weaker assertion
such as `ends_with` gain no display-string pin support here. Unsafe edits in a
Display write tail also cannot recover discriminator credit through generic
token overlap, and no owner-pin route (helper-chain entry, field projection or
fallible receipt) confirms such an edit without an admitted output pin. Existing assertion provenance stays intact: the missing evidence
is observation of the changed output, not execution or macro binding. Debug
owners retain their prior handling. No actionability widens.

### A helper whose one caller is a `fmt` (#8999)

A changed helper (`sanitize_label`) that `writeln!(f, "{}", sanitize_label(l))`
calls from a `Display`/`Debug` `fmt` is reached by the same dispatch, one hop
up. `assert_eq!(\n    merge.to_string(),\n    "a b\n"\n)` never names `fmt` or the
helper, so name matching reported "no test is seen calling sanitize_label".

The helper-transfer chain already resolves the helper's unique caller (a helper
with a second caller declines, and a hop through a local macro carries reach
only). When a resolved hop's caller is a `Display`/`Debug` `fmt` owner, a test
that satisfies every link of Behavior items 1-6 for that `fmt` is related to the
helper as `helper_owner_call`, the same relation a test calling a tested
wrapper earns. It is judged separately from a direct call of the entry, so it
rides none of that route's cross-package bypasses: only an in-package test
qualifies.

This is reach only. The helper's result is not shown to be forwarded into the
pinned output, so the finding reads `propagation_unknown`
(`helper_result_not_forwarded`) instead of `weakly_exposed` with "no test is
seen calling", and never `exposed`. A type annotation must be bare or
prefixed only by `crate`, `self` or `super` (`other_crate::Merge` may be a
foreign type that shares the name), and a struct literal or tuple-struct
initializer (`let merge = other_crate::Merge { .. };`) names the local type
under the same prefix rule (bare, `crate`, `self` or `super`; #8999). A factory receiver without a type
annotation (`let merge = Merge::new(..)`) is typed only through that
unique-return rule (#8999); a `Result`/`Option` wrapper, another return
type, a chained `.unwrap()`, a missing or doubled declaration and a trait-only
constructor stay unrelated. The other trait's placeholder, a
different type and a type declared twice stay unrelated.

The route applies the direct route's entity-identity refusals to the `fmt`
caller's type: a `use other::Type as Merge` rename, a same-named type in the
test's module scope or an out-of-line parent module, and a shadowing trait all
refuse. Every hop from the helper up through the `fmt` caller must be a direct
call; a macro-argument hop anywhere on that path carries no formatting reach.
It is evaluated independently of a name-only call: an unrelated same-named
method call (`other.sanitize_label()`) in the test neither blocks nor
downgrades it.

Named limit: the route does not look at how `fmt` uses the helper. A helper
whose result is dropped (`let _ = sanitize_label(l);`) still runs, and one
called only in a conditional branch or loop body may run, so both read as
reached, which is right for reach. A call inside a literal `if false` in `fmt`
itself is also still credited, because the existing helper-transfer chain
records one call site and no control-flow condition (the test-side dead code
above is judged; the `fmt` body is not). This is acceptable only because the
route credits reach and nothing else: such a finding stays
`propagation_unknown`, never `exposed`. A hop through a local
`macro_rules!` identifier argument carries no formatting reach.

Live code only (#8999). A formatting call credits reach only when the test
can run it. A `to_string()` or formatting macro whose result is dropped
(`let _ = format!("{m}");`, `m.to_string();`) or that sits under an unknown
condition (`if cond { .. }`, a `const` flag, a possibly empty `for`) still
runs or may run, so it stays reached. The reader blanks, before any
dispatch check, the code a syntactic reading proves never runs: the block of a
literal `if false` / `while false` (an `else` branch stays live), the
statement under `#[cfg(any())]` or `#[cfg(false)]`, a free nested `fn` whose name
nothing else mentions (methods inside a local `impl`/`trait`/`mod` are called
implicitly or by the harness and stay live), a `let name = |..| ..;` closure whose binding is never
mentioned again (or is `_`), and the statements (not the hoisted items) after a top-level `return`.
Shapes that do not match exactly stay live, so this only removes credit from
dead code. It applies to the direct route and the helper-entry route alike
because both go through `test_dispatches_to_fmt`.

Package scope (#8999). The helper-entry route is in-package only, with the same
rule as the plain inherent method entry: a member-package owner admits only a
test under its package prefix, and a root-package owner admits only a test
outside every member package (a dependent member's test that formats the
re-exported type is not credited). A same-named type with the same `Display`
impl in another package makes the formatted type declared twice, so no test
relates in either package.

## Required Evidence

- Positive: Debug and Display via `format!`, `write!`, `println!`,
  `.to_string()`, inline `{err:#?}`, positional, named and width forms.
- Negative: wrong trait, unrelated type, unknown value type, `$` width,
  typed-after-macro, ambiguous impls, same-named type elsewhere, rebound
  macros, associated const/factory, inherent `to_string`, turbofish/closure
  comma argument shifts.
- Discriminator: Display and Debug impls of one type are told apart.
- Live code (#8999): positive for a dropped `let _ = format!(..)`, a dropped
  `m.to_string();`, an unknown condition, the `else` of a literal `if false`,
  a called closure and a called nested fn; negative for a literal `if false`
  or `while false` block, `#[cfg(any())]`, code after `return`, a never-called
  closure and a never-called nested fn, on the direct route and on the
  helper-entry route (`crates/ripr/tests/fmt_dispatch_live_reach.rs`).
- Package scope (#8999): positive for the owner's own package test (root
  unit and integration tests, member tests); negative for a dependent member
  package's test of a root owner, the root package's test of a member owner,
  and a same-named twin type in an unrelated package
  (`crates/ripr/tests/fmt_helper_entry_package_scope.rs`).
- Helper entry (#8999): positive for `to_string()` in a multi-line
  `assert_eq!` on an annotated or struct-literal receiver; negative for an
  unproven factory receiver (positive for an indexed `Self`-returning inherent constructor), another type, the other trait, a doubly declared
  type, a path-qualified foreign annotation or initializer, a macro-argument hop, a test in a
  separate file (positive), a second caller of the helper, and a non-Display/Debug `fmt` entry.

## Non-Goals

- A variant imported by name (`use Error::DecodeError;` then
  `DecodeError(inner)`) is indistinguishable from the type by syntax and is
  a known limitation of the bare-name form and is not refused.

- Derived `Debug`, trait objects, generic dispatch, and values returned by
  calls whose type needs inference.

## Acceptance Examples

1. `let err = DecodeError::InvalidLastSymbol(1, b'0');` then
   `assert_eq!(format!("{:?}", err), "...")` relates the test to
   `impl Debug for DecodeError::fmt` as `direct_owner_call`.
2. The same test against `impl Display for DecodeError::fmt` relates nothing.
3. A second `DecodeError` declared elsewhere, or a local
   `macro_rules! format`, relates nothing.

## Test Mapping

- `crates/ripr/tests/fmt_dispatch_live_reach.rs`: public-API controls for
  dropped, conditional and provably dead formatting calls.
- `crates/ripr/src/analysis/classify/related_tests/fmt_live.rs`: the `tests`
  module, one test per dead-code shape and its live counterpart.
- `crates/ripr/tests/fmt_helper_entry_package_scope.rs`: multi-package
  workspaces for the in-package rule.
- `crates/ripr/tests/fmt_helper_entry_dispatch.rs`: public-API controls for
  the helper-entry route, asserting the rendered `propagation_unknown` class.
- `crates/ripr/src/analysis/classify/related_tests/fmt_dispatch.rs`: the
  `tests` module, one test per Required Evidence bullet, plus the Display
  string pin tests.
- `crates/ripr/src/analysis/classify/propagation_witness.rs`:
  `fmt_owner_tail_line_reads_only_a_write_into_the_formatter_parameter`.
- `fixtures/rust-verdict-corpus` case `spec0236-seam-boundary-via-display-impl`
  (helper `suffix` called only by `Display::fmt`; verdict `gap` -> `limited`,
  inside its acceptable set).
- `fixtures/rust-verdict-corpus` case `idiom-display-string-pin` (runtime
  truth `discriminated`, verdict `credited`).

## Implementation Mapping

- `crates/ripr/src/analysis/classify/related_tests/fmt_live.rs`: `live_body`.
- `crates/ripr/src/analysis/classify/related_tests/fmt_dispatch.rs`:
  `FmtContext`, `test_dispatches_to_fmt`, `macro_dispatch`, `args_dispatch`,
  `expr_has_type`.
- `crates/ripr/src/analysis/classify/related_tests.rs`: `fmt_entry_hops` and
  `dispatches_to_helper_fmt_entry` (helper-entry route) and the candidate
  prefilter for a hop whose caller is a `fmt` owner.
- `crates/ripr/src/analysis/classify/related_tests.rs`: candidate prefilter,
  `calls_owner` and `owner_call_relation_reason` wiring.
- `crates/ripr/src/analysis/classify/related_tests/fmt_dispatch.rs`:
  `assertion_pins_display_string`.
- `crates/ripr/src/analysis/classify/propagation_witness.rs`:
  `format_tail_witness` reads the `write!(f, ..)` tail of a `fmt` owner.
- `crates/ripr/src/analysis/classifier/evidence.rs`: admits the Display
  string pin through the owner-return-pin closure for a `fmt_write_owner_tail`
  witness.
- `crates/ripr/src/analysis/classify/reveal.rs`: a `fmt` owner skips the
  same-name import defeat.

## Metrics

- `unit_test_pass_rate`: the fmt dispatch tests are required Rust tests.
