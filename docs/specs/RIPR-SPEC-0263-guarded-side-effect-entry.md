# RIPR-SPEC-0263: Guarded side effect no test input enters

Status: proposed

Owner: product-analysis

Created: 2026-10-13

Linked issues: [#7132](https://github.com/EffortlessMetrics/ripr-swarm/issues/7132),
[#7964](https://github.com/EffortlessMetrics/ripr-swarm/issues/7964).

Support-tier impact: None

The rule only lowers a reading from `exposed` to `weakly_exposed` when the
related tests provably never run the changed line. It adds no evidence class
and changes no support tier.

Policy impact: no dependency, network, workflow or security-policy changes.

## Problem

A changed side effect inside an `if` runs only for inputs that make the
condition true:

```rust
pub fn add(v: &mut Vec<u32>, x: u32) {
    if x > 10 {
        v.push(x);          // changed
    }
}

#[test]
fn adds_nothing_for_small_values() {
    let mut v = Vec::new();
    add(&mut v, 4);
    assert_eq!(v, Vec::<u32>::new());
}
```

The only related test passes 4, which never enters `x > 10`, so no mutant of
the push executes. Every runtime mutant passes. Before this spec, the
direct-collection observer credited `assert_eq!(v, Vec::<u32>::new())` with
revealing the push, and infection read `Yes` because a reachable test exists.
The `side_effect` and `call_deletion` findings therefore read `exposed`. A
wrong `exposed` is worse than a missed one.

RIPR-SPEC-0229 already applies this reasoning to a `match` arm that no test
input selects, and it excludes guards. This spec is the `if`-guard
counterpart for side effects.

## Behavior

1. **Scope.** The rule applies to `side_effect` and `call_deletion` probes
   whose changed line sits directly in the block of an `if` (or `else if`).
   The `if` must open the innermost `{` around the line, so a line in a
   nested closure, loop or `else` block is out of scope. A changed line
   that holds a brace of its own (`} else { small(v); }`,
   `if y { v.push(x) }`) is unreadable.
2. **Readable guard.** The condition must be exactly one comparison
   (`<`, `<=`, `>`, `>=`, `==`, `!=`) between a named owner parameter and an
   integer bound, in either order. Redundant outer parentheses are
   allowed. The bound is an integer literal, or an upper-case name
   (`x > LIMIT`, #7964) that reads only when all of these hold:
   - the indexed files declare exactly one `const` or `static` of that name;
   - it is a file-level `const LIMIT: <integer type> = <integer literal>;`
     in the owner's own file;
   - that file has no import binding the name and no macro that may declare
     items the owner sees;
   - no function of the owner's name in that file sits inside an inline
     `mod`;
   - the guard is the owner's only mention of the name, so no parameter,
     `let` or nested item shadows it.

   The following shapes are unreadable:
   - a second operator (`&&`, `||`, `!`, a shift, arithmetic);
   - a named bound outside the rule above (a `static`, a computed
     `const`, two definitions, an imported name);
   - a method call or field (`x.len() > 10`);
   - `if let`.
3. **Unchanged parameter.** The parameter must be immutable (no `mut`
   pattern or `&mut` type). The owner text before the `if` must not:
   - contain a loop;
   - name the parameter in a binding, assignment, compound assignment or
     call-argument position.

   Telling a call argument from a pattern position needs a parser, so both
   refuse. The owner must not name itself past its signature, and no
   function it reaches through its calls may name it: a recursive call can
   enter the guard with a value no test passes. The self-mention check is
   textual, so a method that calls a same-named method on a field
   (`self.items.push(x)` inside `fn push`) is refused too; that is a known
   over-refusal, never a false reading. An owner, or a function it
   reaches, that invokes a macro other than the standard assertion and
   formatting macros is refused too, since the expansion may call the owner
   back. Those macros are trusted by name only when no indexed file binds
   one of their names to something else: a `macro_rules! println` definition,
   an import, or a glob or `#[macro_use]` that may bring one in, read by the
   parser-backed macro binding scan. A path-qualified call is trusted only
   under `std::`, `core::` or `alloc::` (`helpers::println!` is unread).

   Every mention of the owner in any indexed file, other than its own `fn`
   header and plain `use` paths, must be a call held by a related test's
   body, since only those calls are read. Any other mention is refused,
   since it may run the owner with an input no test shows. That covers a
   production caller, a `Drop` or operator impl, a closure in a `static`, a
   renamed import (`use crate::add as push_big;`), a stored pointer
   (`[add]`) and an argument (`apply(v, add)`). Comments and strings are
   masked, so a doc comment that names the owner is refused separately: a
   doctest runs under `cargo test` with whatever input it passes. A doc
   attribute that includes a file (`#![doc = include_str!(..)]`) and any
   `include!` are refused too, since the included code is not read. An
   unrelated test that calls the owner, or a local, field or doc text
   sharing its name, also refuses; that is a known over-refusal.
4. **Every test misses the guard.** For each related test, all of these
   must hold:
   - the test names the owner;
   - every mention of the owner is a readable direct call (the
     RIPR-SPEC-0229 owner-call reading);
   - every call passes an integer input at the parameter's position;
   - that input leaves the comparison false.

   An integer input is a literal, or a test-local name (#7964) that reads
   only when all of these hold:
   - it is bound by an immutable `let n = <integer literal>;` (a plain type
     annotation allowed);
   - that `let` is a statement of the test's own body block and the name's
     first mention;
   - its only other mentions are owner-call arguments at the parameter's
     position;
   - the test declares no nested `fn`.

   A mutable, shadowed or computed binding (`let n = make();`, `let n = 2 +
   2;`), or a name used anywhere else, stays unread.

   The same refusals as RIPR-SPEC-0229 apply. A partial workspace index, a
   same-named function elsewhere, a test that may reach the owner through a
   helper or an unread macro (in its own body or in a helper it reaches),
   or a test file that imports a foreign
   same-named function or whose package defines one leaves the guard
   unnamed.
5. **Reading.** When rule 4 holds, the probe gets a missing discriminator
   whose reason opens with `No related test call enters guard` and names the
   guard (`if x > 10`) and the observed inputs (`n = 4` for a let-bound
   name). Infection reads `Weak`, so
   the finding reads `weakly_exposed` instead of `exposed`, and the
   recommended next step asks for a related test input that enters the
   guard rather than a stronger assertion. Related tests
   that miss show `missing_input`, and the human text says "no test input
   enters `if x > 10`" and "use an input that enters `if x > 10`".
6. **Fail closed.** Any unreadable piece keeps today's reading. The rule
   never raises a verdict, and an unknown never names the guard.

## Required Evidence

- An end-to-end check names the guard and reads `weakly_exposed` for
  `x > 10`, `10 < x` and `(x >= 11)` with input 4.
- The same check names the guard for `let n = 4; add(&mut v, n);` and for
  `x > LIMIT` with `const LIMIT: u32 = 10;` and input 4.
- The same check keeps `exposed` for input 11, and names no guard when one
  call enters, when the input is computed (`2 + 2`), when a let-bound input
  enters, is mutable, is shadowed or is rebound by a closure parameter,
  when the guard is unreadable (`&&`, arithmetic), when a named bound is
  entered, computed, a `static`, defined twice, imported or declared by a
  macro, when the owner recurses, when the changed line closes the `if`
  block on the same line, or when a test reaches the owner through a
  helper.
- A labeled corpus case with runtime truth (both push mutants pass) reads
  the ideal `gap` instead of `false_exposed`.

## Non-Goals

- Evaluating compound conditions, computed constants, `static` bounds,
  imported or associated constants, computed inputs or helper-chain
  arguments.
- Applying the rule to return-value, predicate or field-construction probes.
- Reading `else` branches, `match` guards, loops or early returns.
- Changing the direct-collection observers themselves: they still say what
  an assertion observes; this rule says the tests never run the change.

## Acceptance Examples

1. `if x > 10 { v.push(x) }` changed, test `add(&mut v, 4)` with an
   empty-vector equality: `weakly_exposed`, missing input `if x > 10`.
2. The same with `add(&mut v, 11)` and `assert_eq!(v, vec![11])`: `exposed`
   (unchanged).
3. Calls `add(&mut v, 4)` and `add(&mut v, 11)` in one test: no guard named.
4. `let n = 4; add(&mut v, n);` with an empty-vector equality:
   `weakly_exposed`, missing input `if x > 10`, observed `n = 4` (#7964).
   `let mut n = 4;`, a shadowing `let n`, or `let n = 2 + 2;`: no guard
   named.
5. `if x > LIMIT` with `const LIMIT: u32 = 10;` in the owner's file and input
   4: `weakly_exposed`, missing input `if x > LIMIT` (#7964). A `static
   LIMIT`, a computed `const`, two definitions or an imported `LIMIT`: no
   guard named.
6. An owner that calls itself (`walk(v, depth + 1)`) after the guard, or
   calls a helper that calls it back: no guard named (recursion). The same
   holds when a macro the owner, a helper it calls or a test helper invokes
   calls it back (including a local macro named `println`). It also holds
   when a test calls the owner through a value (a renamed import, a
   `static` pointer or closure, a `const` table), and when production code
   calls it (a `Drop` or `AddAssign` impl).

## Test Mapping

- `crates/ripr/tests/guarded_side_effect_unentered.rs` covers acceptance
  examples 1 to 6 end to end:
  - `a_literal_that_misses_the_guard_names_it_and_is_not_exposed`
  - `an_input_that_enters_the_guard_keeps_the_exposed_reading`
  - `one_entering_call_or_one_unread_input_names_no_guard`
  - `a_let_bound_literal_that_misses_the_guard_names_it`
  - `a_named_const_bound_that_the_input_misses_names_the_guard`
  - `an_unreadable_guard_names_nothing`
  - `recursion_a_same_line_else_or_a_helper_names_no_guard`
- `analysis::classify::arm_selection::guard_entry::tests` covers the guard
  parser, the comparison semantics and the rebinding refusals.
- The corpus case `rust-guarded-push-unentered-branch` is the labeled
  runtime witness; `spec0263-let-bound-input` and
  `spec0263-named-bound-guard` witness examples 4 and 5.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/arm_selection/guard_entry.rs::GuardEntry`
  reads the guard and judges each test's owner calls, including let-bound
  inputs; `named_integer_bound` resolves a named bound.
- `crates/ripr/src/analysis/classify/activation.rs::missing_guard_entry_discriminator`
  applies the RIPR-SPEC-0229 refusals and emits the fact.
- `crates/ripr/src/domain/probe.rs::GUARD_UNENTERED_REASON_PREFIX` and
  `is_unselected_input_reason` share the reason between the analyzer, the
  infection stage (`classify/infection.rs`), the foreign-identity
  withholding (`classifier/evidence.rs`) and the renderers
  (`output/human/explain.rs`, `output/related_test_miss.rs`).

## Metrics

The Rust verdict corpus false-verdict count drops by one
(`rust-guarded-push-unentered-branch` moves from `false_exposed` to the ideal
`gap`), with no other row moved. The #7964 amendment moves
`spec0263-let-bound-input` and `spec0263-named-bound-guard` from
`false_exposed` to `gap` in the same way. The named-bound subject's
`LIMIT` doc comment was reworded so it no longer names the owner, since a
doc comment naming the owner withholds the reading; it holds no code
block, so its runtime labels are unchanged.
