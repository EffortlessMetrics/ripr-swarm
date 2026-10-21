# RIPR-SPEC-0293: Display and Debug fmt dispatch reach

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
6. A named operand is typed only when the test body binds that name exactly
   once (the typed `let`). Any other binder (`if let`, `while let`, `for`,
   closure or `fn` parameter, `match` arm, another `let`), even one opening
   the macro's own scope, leaves it untyped.

Existing shadow checks for the owner's type and trait still apply. Derived
`Debug` has no indexed `fmt` owner and is out of scope.

## Required Evidence

- Positive: Debug and Display via `format!`, `write!`, `println!`,
  `.to_string()`, inline `{err:#?}`, positional, named and width forms.
- Negative: wrong trait, unrelated type, unknown value type, `$` width,
  typed-after-macro, ambiguous impls, same-named type elsewhere, rebound
  macros, associated const/factory, inherent `to_string`, turbofish/closure
  comma argument shifts.
- Discriminator: Display and Debug impls of one type are told apart.

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

- `crates/ripr/src/analysis/classify/related_tests/fmt_dispatch.rs`: the
  `tests` module, one test per Required Evidence bullet.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/related_tests/fmt_dispatch.rs`:
  `FmtContext`, `test_dispatches_to_fmt`, `macro_dispatch`, `args_dispatch`,
  `expr_has_type`.
- `crates/ripr/src/analysis/classify/related_tests.rs`: candidate prefilter,
  `calls_owner` and `owner_call_relation_reason` wiring.

## Metrics

- `unit_test_pass_rate`: the fmt dispatch tests are required Rust tests.
