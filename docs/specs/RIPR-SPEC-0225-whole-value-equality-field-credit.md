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

- #7858 (fields of a returned array element; decision 8)

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

8. **Fields of a returned array element (2026-10-15, #7858).** When the
   owner is a free function whose whole body is one array literal of struct
   literals of the same type, none with a `..base` and no attribute anywhere
   in the array (a `#[cfg]` element may be compiled out and shift every
   later index), and the changed field `f` is built in exactly one element
   `k`, a related test pins `f` of element `k` when all of these hold:

   - the struct is declared exactly once in the owner's file, in the
     owner's own inline module (a local item takes the name from every
     import, so a macro-generated type or a decoy in another module cannot
     stand in), with no attribute beyond derives, docs and lint levels, no
     generic parameter and no attribute on an enclosing module, and no other
     indexed file declares a type of that name;
   - `f` is declared once, without attributes, as a primitive integer
     (`u8`..`u128`, `usize`, `i8`..`i128`, `isize`) that no workspace type,
     alias, rename or foreign import rebinds in the owner's file, and no
     workspace macro may declare: every file parses, every item-level macro
     call names a macro the workspace defines (or `thread_local!` or
     `compile_error!`), and no macro body or argument that holds `struct`,
     `enum`, `union` or `type` also holds the name or a `$` metavariable.
     So `==` against an integer literal is the built-in comparison and not
     a workspace `PartialEq`. Both forms also need the closed workspace
     the membership bullet below defines: a workspace macro (`use .. as $n`),
     an outside crate's macro or a module in a directory the index skips
     (`src/fixtures/`) may otherwise declare a local `u32`. An outside
     procedural macro's output is not read; a derive or attribute macro that
     declares a type named after a primitive is outside this decision;

   - the owner's name is the name of no other indexed function, because the
     owner call is matched by name and a nested module's `super::rows` or a
     test file's local `fn rows` could take it;
   - the test's only attribute is exactly `#[test]` (an empty attribute
     list, as the lexical fallback and harness-registered tests carry,
     gives no credit), and no token of its body, macro arguments included,
     is `return`, `break`, `continue` or `?`, and it declares no item;
   - the pinning assertion is the test's first statement (or its tail),
     preceded at most by a plain immutable `let` of the owner call that the
     whole test body binds nowhere else, so nothing can rebind the receiver,
     exit or settle the test before it runs;
   - the receiver is that binding or a direct call of the owner by name (or
     `super::`, `crate::`), the call's arguments are literals or paths, and
     the shared free-call binding authority that targeted owner selection
     uses (`targeted_free_call_reaches_owner`) places the call on the owner:
     no `const`, `static`, import or item in the test's scope takes the
     name. The receiver's own `let rows = rows(..)` is read with its binding
     renamed, since the call runs before the binding takes the name;
   - the macro is a bare `assert_eq!` or `assert!` (a qualified path may be
     any crate's macro), and the shared assertion-context validator that
     owner pins use admits the assertion on that line (a local or imported
     rebinding of the macro, an unresolved module composition or an escape
     the oracle scan models refuses it);
   - it is `assert_eq!(recv[k].f, literal)`, in either order, with `k` an
     integer literal; or `assert!(recv.iter().any(|r| r.f == literal))`
     (`into_iter`, literal first, no message), where every other element
     builds `f` from a decimal integer literal different from `literal`,
     every literal in range for `f`'s type (an out-of-range literal under
     `overflowing_literals` wraps onto another value), and the workspace is
     closed for iteration: every package holding an indexed file depends
     only on others of them, through plain unrenamed `path` dependencies
     (directly or through `[workspace.dependencies]`), none is a proc-macro
     library, and no manifest `[patch]`/`[replace]` or `.cargo` configuration
     sits between them and the root; the workspace defines no macro
     (`macro_rules!` or `macro`); every out-of-line `mod name;` resolves by
     default to an indexed file; every indexed file parses, nothing sits outside
     the index (no non-UTF-8 source, include limitation, `include!` macro
     however qualified, or attribute naming `path`, `cfg_attr` included),
     every `use`, macro path and attribute path is rooted in `std`, `core`,
     `alloc`, `crate`, `self`, `super` or a workspace package, there is no
     `extern crate`, no macro body or argument holding `use`, `extern` or
     `include`, no `fn $name` in a macro, and
     every `fn iter`, `fn into_iter` or `fn any` (comments skipped) is an
     indexed free, local or inherent function. An outside crate's trait or a
     generated method could otherwise make the membership iterate something
     other than the returned elements. Most real workspaces import outside
     crates, so both forms credit only small closed ones.

   The membership needs the last condition because it holds through any
   element: with `[Row { total: 33 }, Row { total: a + b }]`,
   `rows(30, 3).iter().any(|r| r.total == 33)` passes whatever the second
   element holds, so it observes nothing. A computed sibling, an index of
   another element, `all`, `!=`, an ordering, a negated membership, a typed
   or pattern closure parameter, a `let mut` or shadowed receiver, a
   `#[should_panic]` or `#[ignore]` test, an assertion message, a statement
   before the assertion (a macro that may rebind the receiver,
   `std::process::exit`), a locally defined `assert_eq!`, a method owner, a
   `const rows: fn(..)` in the test module, a trait `iter` on the array
   (written plainly, behind a comment, or by a macro), an outside crate's
   trait, a `#[path]` module (also behind `cfg_attr`), a macro body that
   imports, a qualified `std::include!`, a wrapping sibling literal, a
   `#[cfg]` element,
   a field type with its own `PartialEq`, a macro that declares a type
   named after the field's primitive or is handed `fn` to write `iter`, an
   item-level macro the workspace does not define, any outside or renamed
   dependency (`alloc = { package = .. }`) and a decoy declaration of the
   struct all keep the `FieldValue` missing discriminator. Owner calls get the same
   same-name import and cross-package defeats as reveal's owner pins. A
   `vec![..]` owner, a `let r = &rows[1]` or `let [_, r] = rows` read and a
   collected scalar owner are not covered. Rejected: crediting any
   `.any(|r| r.f == literal)` on the owner's result, which credits the
   decoy. Verdict-corpus case `spec0231-oracle-any-field-eq-literal`
   (RIPR-SPEC-0231 example 28's membership on a returned array) moves from
   gap to credited.

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
- Decision 8: `crates/ripr/tests/returned_element_field_pin.rs` (the
  membership and index pins read `exposed`; the `total: 33` decoy, a
  computed sibling, another element's index, a shadowed receiver, a faked
  or skipped assertion, a nested module's same-name `rows`, a callable
  `const`, a trait `iter`, a `#[cfg]` element, a custom `PartialEq`, the
  open-world memberships and a decoy struct declaration keep the gap)
  and the unit tests in
  `crates/ripr/src/analysis/classify/element_field_pin.rs`.
- Planned: oracle classifier unit tests for braces outside struct-literal
  operands.

## Implementation Mapping

- `crates/ripr/src/analysis/extract/oracles/classify.rs` and `patterns.rs`:
  parser-backed `whole_object_equality`.
- `crates/ripr/src/analysis/classify/activation.rs`: clear `FieldValue` for a
  named field in an admitted whole-value literal.
- `crates/ripr/src/analysis/classify/owner_pin/expected_binding.rs`: bounded
  adjacent expected binding admission before the existing literal gates.
- `crates/ripr/src/analysis/classify/owner_pin.rs`: reuse call identity and
  execution admission (`establish_whole_value_field`, `whole_value_tail`,
  `WholeValueField::admits`, `bound_owner_initializer`); decision 6 adds
  `returns_other_variant` and `unwrapped_call`, covered by
  `an_unwrapped_ok_tail_with_early_err_returns_pins_its_field`. Decision 7
  adds `expected_item_literal` in `expected_binding.rs`, covered by
  `an_expected_helper_or_default_base_reads_as_its_literal`.

- `crates/ripr/src/analysis/classify/element_field_pin.rs`: decision 8
  (`observes_returned_element_field`), consumed in
  `crates/ripr/src/analysis/classifier/evidence.rs` where it clears the
  `FieldValue` missing discriminator next to reveal's owner-pin credit.

## Metrics

- `field_construction_whole_value_credit`: findings confirmed by a whole-value
  literal naming the changed field.
- `whole_object_equality_lexical_only`: assertions that earned the kind from a
  brace alone; must be zero.
