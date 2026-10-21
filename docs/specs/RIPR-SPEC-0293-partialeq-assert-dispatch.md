# RIPR-SPEC-0293: Comparison assertions reach a changed user PartialEq

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

- #8999 (item 4: `assert_eq!` over a type whose `PartialEq::eq` changed is not
  related to the changed `eq`)

Linked PRs:

- #9027

Support-tier impact:

- None. A related-test relation is added under existing vocabulary; no exposure
  class, command or output field is added.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- No new exposure class, oracle kind, severity or stage-state value. The
  relation reason is the existing `direct_owner_call`.

## Problem

`assert_eq!(a, b)` expands to a comparison that runs `PartialEq::eq` of the
operands' type without spelling `eq`. A test comparing two values of a type
whose `impl PartialEq for T { fn eq }` changed therefore called the changed
function, but no call fact named it, so the finding read as unreached. The
same run also let a fixture-like word in a test that merely shared a file with
the owner make the `eq` probe `fixture_opaque`.

## Behavior

### Relation

A same-file test is related to the changed `eq`/`ne` of a non-generic
(lifetime parameters allowed) `impl PartialEq for T` as a `direct_owner_call`
when all hold:

- `PartialEq` is the standard trait: no workspace file declares a
  `trait PartialEq` and the owner's file imports none by a non-standard path
  or glob.
- The assertion is `assert_eq!`/`assert_ne!` or a `debug_` twin, outside any
  opaque property-test macro (`proptest!`, `quickcheck!`, ...), and no user
  macro of that name is defined in any workspace file, imported by `use`, or
  brought in by `#[macro_use]`.
- Both compared operands have type `T` established by syntax or a resolved
  declaration: the unit struct, `T { .. }`, `T(..)`, `T::Variant(..)`,
  `T::assoc(..)` whose indexed declaration returns `Self`/`T`, or a binding
  whose last `let` is annotated `: T` or initialised by one of those. A
  constructor-shaped name (`new`, `default`) is not a return type.
- No scope between the test and the file declares or renames another `T`; the
  file declares `T` once; and no `use` may name another `T` (`super::T` and,
  from a crate-root file, `crate::T` are accepted; other paths and globs are
  refused).
- No binding of an operand is rebound by a `for`/`if let`/`while let`/`let`
  pattern (including struct patterns) or a closure parameter.
- `assert_ne!` reaches `eq` only through the default `ne`, so a custom `ne` in
  the same impl withholds it; an owner `ne` is reached by `assert_ne!` alone.

The relation is evaluated independently of a name-only `eq` call elsewhere in
the test; the strongest valid relation wins. It relates the test to the owner
only; the assertion is credited or refused by the ordinary oracle pipeline and
no exposure class is raised.

### Fixture-opaque scope

For a standard `PartialEq` `eq`/`ne` owner only, when some related test
establishes reach, the `fixture_opaque` stop reason reads only the reaching
tests and tests related by fixture-owner affinity; with no reaching test, and
for every other owner, every related test is read as before.

### Limits (fail closed)

A match-arm pattern or guard naming an operand, and more than one `let` of
it before the assertion, make the binding ambiguous and are refused. Not
read: `cfg`-gated impls, a comma inside generic angle brackets in an
operand (the split fails to resolve),
constructor return types beyond a `Self`/`T` signature, generic impls, and
operands typed through any other expression. Any such case yields no relation.

## Required Evidence

- Positive: two typed operands, a literal operand, `assert_ne!`, `debug_assert_eq!`.
- Negatives: unresolved operand, another type returned by `new`, local
  `trait PartialEq`, test-module shadow, two `T` declarations in one file,
  `use` of another `T`, user `assert_eq` macro, pattern, closure, `if let` and match-arm rebinding, ambiguous
  shadowing `let`, imported or `#[macro_use]` assert macros (any workspace
  file), `use other::PartialEq` or glob, `foo_std::` paths, assertion
  inside a property macro, generic impl, custom `ne`.
- Mixed: a name-only `eq` call beside a macro dispatch keeps the direct relation.
- Classes equal those of an explicit `a.eq(&b)` call.
- The fixture narrowing is scoped to `PartialEq` owners with a non-`PartialEq`
  control.

## Test Mapping

- `crates/ripr/tests/partialeq_assert_dispatch.rs` (all controls named
  in Required Evidence; registered in `.ripr/traceability.toml`)
- `crates/ripr/src/analysis/classify/related_tests/equality_dispatch.rs`
  unit tests (operand typing, import refusal, rebinding, macro splitting)

## Acceptance Examples

```rust
let a = Key::new("a", 1);
let b = Key::new("a", 2);
assert_eq!(a, b); // related to the changed `impl PartialEq for Key::eq`
```

## Non-Goals

- No new exposure class, relation reason, oracle vocabulary or output field.
- No type inference beyond the syntactic and declared-return forms above.
- Operands containing a top-level comma inside generic angle brackets
  (`Vec<u8, A>`) split wrongly and fail to resolve: deliberate fail-closed.
- No reading of `cfg`-gated impls or generic impls.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/related_tests/equality_dispatch.rs`
  owns the reading: owner shape, std-trait check, operand typing, shadow,
  import, macro and rebinding refusals.
- `crates/ripr/src/analysis/classify/related_tests.rs` wires the relation as
  an independent signal beside `calls_owner` and gates the prefilter bucket.
- `crates/ripr/src/analysis/classifier/finding.rs` scopes the
  `fixture_opaque` scan for `PartialEq` owners.

## Metrics

- `unit_test_pass_rate` on the registered tests.
