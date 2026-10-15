# RIPR-SPEC-0225: Whole-value equality credit for constructed fields

Status: proposed

Owner: product / analysis

Created: 2026-10-04

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- None yet

Linked PRs:

- None yet

Support-tier impact:

- No tier change. A Rust `field_construction` finding can read `exposed` when
  a related test compares the owner's whole result against a literal that
  names the changed field, and the type's equality is the derived one. The
  `whole_object_equality` oracle kind stops being assigned from a `{`
  character alone. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No output schema version bump. The seam cache schema moves (1.48) because
  cached verdicts change.

## Problem

Comparing a whole returned value is the most common way Rust tests pin a
constructor:

```rust
pub fn build(n: u32) -> Config { Config { retries: n + 1, name: "x".into() } }

#[test]
fn builds() {
    assert_eq!(build(3), Config { retries: 4, name: "x".into() });
}
```

The diff changed `retries: n + 2` to `retries: n + 1`, and any wrong value
for `retries` fails this test. ripr reports one
`field_construction` finding and reads it `weakly_exposed`. The oracle is
classified `whole_object_equality` (strong), reveal matches the `retries`
token, and then activation keeps a `FieldValue` missing discriminator because
the assertion neither contains `retries: n + 1` nor reads `.retries` on the
owner result (`analysis/classify/activation.rs`,
`missing_field_value_discriminator` and `reads_owner_result_field`). The
evidence classifier downgrades the finding to weak. A real, ordinary pin is a
false gap. The "Fill spec-defined corpus cases" ledger crate reproduces it
with `Header { kind: 7, len: 3 }` against mutant truth.

RIPR-SPEC-0005 already says credit requires that "the oracle structurally
names the exact constructed field through a member access or record
field/pattern". A struct literal naming the field is a record field, so the
current behavior under-delivers that spec.

The opposite risk sits in the oracle classifier. Any `assert_eq!` or
`assert_ne!` line containing `{` is classified `whole_object_equality` with
strong strength (`analysis/extract/oracles/classify.rs` and
`patterns.rs`, `is_whole_object_equality_assertion`). A brace in a format
string or a closure qualifies. No spec governs that rule, and a derived
`PartialEq` is never checked, so a manual `impl PartialEq` that ignores the
changed field would be credited the moment field credit is added.

## Behavior

### Whole-value oracle

An assertion is `whole_object_equality` only when the parser shows an
`assert_eq!` or `assert_ne!` operand that is a struct literal, or a single
tuple-variant constructor (`Ok`, `Some`, `Err` or an enum tuple variant)
wrapping one struct literal. Braces anywhere else do not make the kind.
Assertions that no longer qualify fall to the kind their other text earns.

### Field credit

A changed `field_construction` probe for field `f` of type `T` in a struct
literal the owner returns is confirmed by a related test when all of these
hold:

1. One `assert_eq!` operand is the owner's call, admitted by the same call
   identity and execution gates as RIPR-SPEC-0197, or an immutable `let`
   binding in the same test whose initializer is that call, with no
   `&mut` borrow, shadowing or interior-mutability write of the binding
   between the call and the assertion. A `let mut` binding gives no credit.
2. The other operand is a struct literal of `T` (wrapped at most once as
   above, matching any wrapper on the owner's return; a wrapper other than
   `Ok`, `Some` or `Err` must itself be a workspace enum that meets rule 3)
   that names `f`
   explicitly with an independent expected value: an expression built only
   from literals, constants, standard-library or prelude constructors and
   conversions on them, and workspace struct, tuple-struct or enum-variant
   literals whose leaves are themselves admissible (`4`, `MAX`, `"x".into()`,
   `Some(4)`, `Retries(4)`, `Retries { n: Count(4) }`). An expected value
   that names any local binding or calls any function of the workspace
   (including a constructor function such as `Retries::new(4)`, and a
   `From`/`Into` conversion or `Default` whose impl is in the workspace)
   gives no credit, because it can carry the owner's result
   (`retries: c.retries`, `let r = c.retries; .. retries: r`,
   `retries: build(3).retries`). Functional update (`..base`) is not
   admissible at any nesting depth.
3. `T` is a workspace type with a visible `#[derive(PartialEq)]` and no
   manual `impl PartialEq for T` in the workspace.
4. The type of `f` compares by value: a primitive, `String`, `&str`, or a
   standard collection or `Option`/`Result` of such types, or a workspace type
   that itself meets rule 3 and whose own field types meet this rule,
   recursively. Any manual `PartialEq` anywhere in that type tree refuses
   credit. No attribute on `f`, `T` or a type in that tree changes equality
   (for example `educe` or `derivative` ignore attributes).

Then activation clears the `FieldValue` missing discriminator for `f` and the
finding may read `exposed`, subject to every other stage.

RIPR-SPEC-0197 rule 6 (clone field pins, #6692) consumes rules 3 and 4 for a
field of a hand-written `Clone::clone` literal compared with its own receiver.
The "No credit" list below, including its bounded separate-binding rule, governs
this spec's literal comparisons; it does not govern rule 6's receiver, whose
independence rule 6 gates itself.

### No credit

The finding does not read `exposed` from whole-value equality when:

- the literal omits `f` through `..base`, or through `..Default::default()`
  outside decision 7's one manual literal `Default`;
- the expected value comes from a helper outside decision 7, an alias, or a
  separate binding outside decision 5's adjacent immutable literal
  admission; it stays `observation_unverified`;
- the compared call is not the owner;
- the assertion is `assert_ne!`: it keeps the `whole_object_equality` kind
  but gives no field credit, because inequality with one wrong value does
  not pin the right one;
- `T` or the type of `f` has a manual `PartialEq`, comes from outside the
  workspace, or its equality cannot be seen (aliases, generics,
  macro-generated types, equality-changing attributes).

A single-field read of the changed field (`assert_eq!(c.retries, 4)`) keeps
crediting as today. A read of a sibling field (`assert_eq!(c.name, "x")`)
keeps the `FieldValue` missing discriminator.

### Decisions

Steven delegated these choices on 2026-10-04 ("make reasonable documented
decisions and proceed"). Each records the adopted option, why, and the
rejected alternative. Any can be reversed later without touching the rest.

1. **Credit whole-value literals at all.** Adopted: yes, as above. This is
   what RIPR-SPEC-0005's "record field/pattern" wording implies.
2. **Wrapper depth.** Adopted: one level (`Ok(T { .. })`). Rejected:
   none, which keeps `parse()`-style tests as gaps.
3. **Equality gate.** Adopted: derived `PartialEq` on `T` and on the
   field's type, fail closed otherwise. When the gate refuses, the finding
   keeps its `FieldValue` missing discriminator and stays a gap
   (`weakly_exposed`); the #5416 unknown-not-a-gap rule 3 does not withhold
   it, because a missing discriminator is named. Rejected: a new
   withholding route with its own `static_limit_kind`, so a refused equality
   gate reads as an analyzer limit instead of a gap.

4. **First implementation scope (2026-10-07, false-verdict push before
   0.11).** Field credit is an owner-return pin
   (`OwnerReturnPin::establish_whole_value_field`), so rule 1's call
   identity and execution gates are RIPR-SPEC-0197's own. Adopted, each
   narrower than the text above and failing closed:
   - the owner's tail is the literal, or `Ok(..)`/`Some(..)` around it
     matching a declared `Result`/`Option` return; a workspace enum wrapper
     is not read yet, so example 14 is refused at the wrapper;
   - the body has no `return` and no macro that may hide an exit, and `?`
     only around a wrapped tail; the changed field's value evaluates on every
     input;
   - a `let` binding of the owner call counts only when the test names the
     binding exactly twice (the `let` and the compared operand), which also
     refuses examples 12, 13 and 15;
   - an expected field value may be a literal (negated too), a tuple or
     array of admissible values, a CamelCase variant or struct constructor of
     such values, or a string literal's `.to_string()`,
     `.to_owned()`, `String::from(..)` or (for a `String` field) `.into()`;
     constants are refused, since a `const` may be computed by a workspace
     `const fn`; generic types and `.into()` inside nested constructors are
     refused too. `String::from(..)` counts only when `String` names the
     standard type workspace-wide: a workspace `mod String`, type, fn,
     const, static, trait, macro or variant of that name, a rename to it, or
     a non-std `use` of it refuses the pin, because the shadowed `from` can
     return the owner's own field and both compared operands then move
     together (#7066 review);
   A field read inside the braces of an expected literal no longer clears the
   `FieldValue` missing discriminator (example 12, which had read `exposed`).
   The parser-backed `whole_object_equality` classifier change is not part of
   this step: field credit parses the expected literal itself, so a brace
   elsewhere cannot earn it.

5. **Adjacent expected literal binding (2026-10-14).** Example 5 admits
   `let e [: T] = T { .. }; assert_eq!(c, e)` when the immutable plain
   identifier is named exactly twice in the test body (its declaration and
   this complete operand), the declaration is the immediately preceding
   statement in the test's own block, and this exact assertion is established
   as executed. The declaration has no attributes, `ref`, subpattern or
   `let ... else`. The initializer must pass the same independent-field,
   wrapper, type and derived-equality gates as an inline literal. Both
   operand orders are admitted. Expected bindings in helper-loaned assertions,
   aliases, mutable/overwritten/shadowed bindings, intervening statements and
   unresolved flow remain refused. This is separate from #7105's refusal of
   an actual-side field read inside braces, which remains unchanged.

6. **Unwrapped `Ok`/`Some` tails and their early exits (2026-10-14).**
   Beside an `Ok(T { .. })` tail, an early `return Err(..)` is admitted, and
   beside a `Some(T { .. })` tail an early `return None`: each exits with the
   variant the tail does not build, as `?` already does, so every `Ok`/`Some`
   the owner returns is the tail literal. Any other `return` (another
   `Ok(..)`, a call, a method chain on `Err(..)`) still refuses, and so does
   an early exit when a workspace file may rebind `Err`/`None` (a `fn Err`,
   a `use .. as Err`), as the tail's `Ok`/`Some` already must not be. The test may
   then compare `owner(..).unwrap()` or `owner(..).expect("..")` (one plain
   string-literal message), inline or through rule 1's once-used `let`, with
   the bare literal `T { .. }`: both panic on `Err`/`None`, so a passing
   comparison reads the `Ok`/`Some` payload, and the expected literal is
   checked as `Ok(T { .. })`/`Some(T { .. })` against every other gate.
   `unwrap_or*`, a computed `expect` message, an unwrapped bare-tail owner and
   an unwrapped value compared with a wrapped literal give no credit. Rejected:
   leaving early-`Err` owners refused, which kept the common
   `let r = ship(..).unwrap(); assert_eq!(r, Receipt { .. })` shape a false
   gap (ledger-receipt-remaining).

7. **Literal helpers and literal `Default` bases (2026-10-14).** An expected
   operand `helper()` reads as the literal its helper returns when `helper`
   is the workspace's only `fn helper`, a free function (file or module
   level) without attributes, `async` or generics, whose body is exactly a
   bare struct literal, and every mention of the name in every file is
   followed by `(`. A literal `T { .., ..T::default() }` (or
   `..Default::default()`) reads as `T` with each omitted field taken from
   the workspace's only `fn default` in an impl of `T`, a manual
   `impl Default for T` whose body is exactly a bare `T { .. }`/`Self { .. }`
   literal without its own base, when the workspace does not rebind
   `Default`. A derived `Default`, a second `default` for `T`, an attributed
   or generic impl, a helper or `default()` with statements, a field with an
   attribute (`#[cfg(..)]`) in either literal, an `r#` helper name, a name
   a macro may declare, or any workspace glob `use` rooted outside `crate`,
   `self`, `super` and the workspace packages (standard library included,
   since the index cannot list what it binds) or passing through a segment
   that is not a workspace `mod` (`extern crate other as ext;` then
   `use self::ext::*;`) gives no credit. `fn r#default` counts as a
   `default`. Both forms apply to the test's own
   `assert_eq!` operands, not to `==` conditions or borrowed
   check-helper assertions. The substituted literal then passes every rule 2 gate, so the
   changed field still needs an independent value, whether written in the
   test or in the helper or `default()`. Rejected: keeping examples 6 and 7
   `weakly_exposed`, which reported a missing discriminator for tests that
   kill both runtime mutants (spec0225-wv-expected-helper,
   spec0225-wv-functional-update-default).
8. **Literal variant-field patterns (#6578, 2026-10-15).** A pattern
   assertion (`assert_matches!`, `debug_assert_matches!`, or
   `assert!(matches!(..))` whose whole condition is the `matches!` call) in
   a test without `#[should_panic]` also clears `FieldValue` for `f` when its
   scrutinee is exactly a direct call of the owner (bare or path-qualified,
   optionally borrowed) whose arguments are all literals, so the test
   cannot hand the owner a variant value it built, and that settles on the
   owner under the RIPR-SPEC-0197 same-name call-identity rule (when that
   rule builds no identity, only a bare or `crate`/`self`/`super` path that
   the test file does not import from another crate, and only when no
   other production function shares the owner's name, since
   `other::owner(..)` may name a foreign namesake and the identity is also
   absent for layouts it cannot classify); its pattern names the variant of the
   struct literal holding the changed initializer, with the same last path
   segment and, when both spell one, the same enclosing segment, exactly
   once; that group binds `f` to a literal (integer, float, `bool`, `char`
   or string, never a range); the pattern holds no `|` alternative and no
   `if` guard; the test body spells no `Name {` head outside its pattern
   assertions; and the owner's literal is the only `Name {` (or
   `Name::<`) head for that variant name in the indexed source outside
   executable test bodies and enum declarations, so the pattern cannot
   observe a value a helper (test-module helpers included), a `From` impl,
   a `const` or `static`, a `macro_rules!` body or a same-named variant of
   another enum built. A production match
   arm or `if let` spelling the head also counts and refuses (fail-closed);
   code the index never reads (`include!`, derive or build-script output)
   and a macro that takes the variant name as a fragment (`$v { .. }`) are
   outside this rule's evidence. A literal pattern compares by value
   without `PartialEq`, so rules 3 and 4 do not apply. A wildcard
   (`f: _`), a binding (`f: p`), a range (`f: 0..5`), `..` without `f`, a
   guard, an alternative,
   another variant, a same-named variant of another enum, a scrutinee that
   is not the owner call (`r`, `owner(..).map(..)`), and `assert_eq!`
   (governed by rules 1-4) give no pattern credit. Rejected: crediting a
   pattern that names `f` in any form, which would credit `f: _`
   (lib-assert-matches-variant-field-pinned moves gap to credited; the
   `{ .. }`, `pos: _`, guarded and alternative controls stay unconfirmed).

## Required Evidence

- The reproduction above reads `exposed`.
- The manual-`PartialEq` control (equality ignores `retries`) does not read
  `exposed`.
- A derived or computed `Default` base, dependent/overwritten
  expected bindings, a computing helper and the non-owner control do not
  read `exposed`.
- The adjacent independent expected literal binding reads `exposed`; unchanged
  literal `4` rejects both `retries: n + 2` and `retries: n` runtime controls.
- A brace inside a format string in an `assert_eq!` message does not produce
  `whole_object_equality`.
- Golden drift: every finding that gains `exposed` names a literal with the
  changed field; every finding that loses `whole_object_equality` is listed
  with the kind it now carries.

## Non-Goals

- No reasoning about `PartialEq` bodies.
- No credit for `Debug`, `Display` or snapshot comparisons (other authorities).
- No change to `return_value` owner-return pins (RIPR-SPEC-0197).
- No credit for nested struct literals more than one level deep.

## Acceptance Examples

The diff changes `retries: n + 2` to `retries: n + 1` in `build`; `Config` derives
`PartialEq` unless stated.

1. `assert_eq!(build(3), Config { retries: 4, name: "x".into() })`: `exposed`.
2. Same, with `impl PartialEq for Config { fn eq(&self, o: &Self) -> bool { self.name == o.name } }`:
   not `exposed`.
3. `assert_eq!(parse("3"), Ok(Config { retries: 4, name: "x".into() }))`,
   `parse` the owner: `exposed`. Under decision 6, `parse` may also
   `return Err(..)` early, and `assert_eq!(parse("3").unwrap(), Config { retries: 4, name: "x".into() })`
   reads `exposed` too.
4. `let c = build(3); assert_eq!(c, Config { retries: 4, name: "x".into() })`: `exposed`.
5. `let c = build(3); let e = Config { retries: 4, name: "x".into() }; assert_eq!(c, e)`:
   `exposed` under decision 5's adjacent immutable binding gates.
6. `assert_eq!(build(3), expected())` with `fn expected() -> Config { Config { retries: 4, name: "x".into() } }`:
   `exposed` under decision 7. A helper that computes or calls the owner:
   `weakly_exposed`.
7. `assert_eq!(build(3), Config { name: "x".into(), ..Config::default() })` with a manual
   `impl Default for Config` returning `Config { retries: 4, name: String::new() }`:
   `exposed` under decision 7. With a derived `Default`: not `exposed`.
8. `assert_eq!(other(), Config { retries: 4, name: "x".into() })`, `other` not the owner: not `exposed`.
9. `let c = build(3); assert_eq!(c.name, "x")`: `weakly_exposed`, `FieldValue`
   missing discriminator kept.
10. `assert_ne!(build(3), Config { retries: 9, name: "x".into() })`: `weakly_exposed`, no field credit.
11. The owner writes `retries: Retries(n + 1)`; `retries` of type `Retries(u32)` with `impl PartialEq for Retries { fn eq(&self, _: &Self) -> bool { true } }`,
    test `assert_eq!(build(3), Config { retries: Retries(4), name: "x".into() })`:
    not `exposed`.
12. `let c = build(3); assert_eq!(c, Config { retries: c.retries, name: "x".into() })`:
    not `exposed`.
13. `let mut c = build(3); c.retries = 4; assert_eq!(c, Config { retries: 4, name: "x".into() })`:
    not `exposed`.
14. The changed literal is in `wrap(n)`, the owner, which returns
    `Wrapped::One(Config { retries: n + 1, name: "x".into() })`; `Wrapped`
    has `impl PartialEq for Wrapped { fn eq(&self, _: &Self) -> bool { true } }`;
    test `assert_eq!(wrap(3), Wrapped::One(Config { retries: 4, name: "x".into() }))`:
    not `exposed`.
15. `let c = build(3); let r = c.retries; assert_eq!(c, Config { retries: r, name: "x".into() })`:
    not `exposed`.
16. The owner writes `retries: Retries { n: Count(n + 1) }`; `retries` of type `Retries { n: Count }` where only
    `Retries` derives `PartialEq` and `Count(u32)` has the manual
    `impl PartialEq for Count { fn eq(&self, _: &Self) -> bool { true } }`,
    test `assert_eq!(build(3), Config { retries: Retries { n: Count(4) }, name: "x".into() })`:
    not `exposed`. The same test with `#[derive(PartialEq)]` on `Count` in place of the manual impl reads
    `exposed`.

## Test Mapping

- Public API/runtime: `crates/ripr/tests/whole_value_expected_binding.rs`
  checks exact field/owner identity, independent literal controls, compiling
  field mutations and dependent/shadowed/overwritten expectations.
- Decision 8: `crates/ripr/tests/variant_field_pattern.rs` (public API:
  the literal pattern reads `exposed`; open, wildcard, guarded,
  alternative and range patterns, a variant a helper, `const` or macro
  also builds, a test-module helper building it, a non-literal call
  argument and a `#[should_panic]` test do not) and the unit tests in
  `crates/ripr/src/analysis/classify/variant_field_pattern.rs`; verdict
  corpus `lib-assert-matches-variant-field-pinned`.

- Existing: `fixtures/observation_verified_field_construction`,
  `fixtures/observation_unverified_field_construction`,
  `fixtures/rust_field_construction_token_coincidence`.
- Verdict corpus: 17 cases `spec0225-*` in
  `fixtures/rust-verdict-corpus/cases/`, one per buildable acceptance
  example, each naming its example in its reasoning and labeled with runtime
  mutant truth.
- Honesty corpus: `rust_owner_pin_whole_value_string_from_shadow` in
  `fixtures/evidence-promotion-honesty-corpus/corpus.json`, the
  should-stay-`weakly_exposed` control for the `String::from` shadow
  refusal (#7066 review).
- Planned: oracle classifier unit tests for braces outside struct-literal
  operands.

## Implementation Mapping

- `crates/ripr/src/analysis/extract/oracles/classify.rs` and `patterns.rs`:
  parser-backed `whole_object_equality`.
- `crates/ripr/src/analysis/classify/activation.rs`: clear `FieldValue` for a
  named field in an admitted whole-value literal.
- `crates/ripr/src/analysis/classify/variant_field_pattern.rs`: decision 8's
  literal variant-field pattern, consumed by `activation.rs`.
- `crates/ripr/src/analysis/classify/owner_pin/expected_binding.rs`: bounded
  adjacent expected binding admission before the existing literal gates.
- `crates/ripr/src/analysis/classify/owner_pin.rs`: reuse call identity and
  execution admission (`establish_whole_value_field`, `whole_value_tail`,
  `WholeValueField::admits`, `bound_owner_initializer`); decision 6 adds
  `returns_other_variant` and `unwrapped_call`, covered by
  `an_unwrapped_ok_tail_with_early_err_returns_pins_its_field`. Decision 7
  adds `expected_item_literal` in `expected_binding.rs`, covered by
  `an_expected_helper_or_default_base_reads_as_its_literal`.

## Metrics

- `field_construction_whole_value_credit`: findings confirmed by a whole-value
  literal naming the changed field.
- `whole_object_equality_lexical_only`: assertions that earned the kind from a
  brace alone; must be zero.
