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
   with the type. Associated consts and factory functions are not typed.
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

- Stimulus. The change must be to string-literal text only (the probe's before
  and after differ only inside string literals). A literal fragment is written
  whatever the test passes in, so an exact pin sees it. A changed argument
  (`self.0 * 2`) can leave the output equal for the one value the test chose, so
  it keeps its prior verdict.

A `format!` or `{}` side, a computed expected string, a weaker assertion such as
`ends_with`, and `Debug` owners are not read here and keep their prior verdict.
This credits only; it widens no actionability.

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
foreign type that shares the name). A factory receiver without a type
annotation (`let merge = Merge::new(..)`), the other trait's placeholder, a
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
whose result is dropped (`let _ = sanitize_label(l);`) or called only in a
conditional branch or loop body still reads as reached, because the existing
helper-transfer chain records one call site and no control-flow condition. This
is acceptable only because the route credits reach and nothing else: such a
finding stays `propagation_unknown`, never `exposed`. A hop through a local
`macro_rules!` identifier argument carries no formatting reach.

## Required Evidence

- Positive: Debug and Display via `format!`, `write!`, `println!`,
  `.to_string()`, inline `{err:#?}`, positional, named and width forms.
- Negative: wrong trait, unrelated type, unknown value type, `$` width,
  typed-after-macro, ambiguous impls, same-named type elsewhere, rebound
  macros, associated const/factory, inherent `to_string`, turbofish/closure
  comma argument shifts.
- Discriminator: Display and Debug impls of one type are told apart.
- Helper entry (#8999): positive for `to_string()` in a multi-line
  `assert_eq!` on an annotated or struct-literal receiver; negative for an
  untyped factory receiver, another type, the other trait, a doubly declared
  type, a path-qualified foreign annotation, a macro-argument hop, a test in a
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
