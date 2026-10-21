# RIPR-SPEC-0295: Rust mock call-count side flip

Status: proposed

Owner: product / analysis

Created: 2026-10-15

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #6578 (runtime-labeled Rust test-library cases; row
  `lib-mockall-grace-boundary-times`)

Linked PRs:

- None yet

Support-tier impact:

- No tier change. The rule reads one more owner and test shape on the
  existing Rust predicate discrimination path.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class or relation
  reason. The seam cache schema version moves because a stored
  classification can change.
- Sits beside RIPR-SPEC-0227 rule 3 (result-side guard flip) and, like it,
  pairs its own oracles, so the #4828 same-test pairing has nothing left to
  establish for a probe this rule confirms.

## Problem

The Rust verdict-corpus row `lib-mockall-grace-boundary-times` scores
`false_actionable`. Both boundary mutants of the guard fail the test:

```rust
#[cfg_attr(test, mockall::automock)]
pub trait Mailer {
    fn send(&self, to: &str, body: &str) -> bool;
}

pub const GRACE_DAYS: u32 = 30;

pub fn remind(mailer: &dyn Mailer, user: &str, overdue_days: u32) -> bool {
    if overdue_days > GRACE_DAYS {
        let body = format!("{user}, your book is {overdue_days} days late");
        return mailer.send(user, &body);
    }
    false
}

#[test]
fn reminds_only_after_the_grace_period() {
    let mut mailer = MockMailer::new();
    mailer.expect_send().with(eq("ann"), always()).times(1).return_const(true);
    assert!(remind(&mailer, "ann", 31));
    assert!(!remind(&mailer, "ann", 30));
}
```

ripr read both assertions as weak relational checks and the mock
expectation as medium, and reported `weakly_exposed`. The owner-return pin
correctly refuses them (RIPR-SPEC-0197: a predicate inside a branch never
pins), because the taken branch returns whatever `send` returns.

The discriminator is the call count. The guarded block calls `send` once
each time it runs, and nothing else calls it, so the test's `send` calls
equal the owner calls the guard takes: one (31). Under `>=` the call at 30
enters the block and makes a second `send` call, which `times(1)` rejects
with a panic. Under `GRACE_DAYS + 1 <` the call at 31 skips the block, and
mockall panics on drop because `send` ran fewer than once.

## Behavior

A predicate probe with an observed reach and a weak discriminator is
discriminated when all of the following hold. Any shape outside them leaves
the evidence unchanged.

1. **Guard.** The owner is a unique free function with no attribute,
   generics, `self` parameter, `async` or `const`. Its first statement is `if <param> <op> <bound> { .. }` with no
   `else`. `<op>` is `<`, `<=`, `>` or `>=`. `<param>` is a parameter of a
   built-in integer type, on either side. `<bound>` is one of two things:
   - an integer literal;
   - the name of a file-level `const NAME: <integer type> = <integer
     literal>;` that is the file's only binding of that name and that the
     owner does not shadow.

   The probe is that condition on its own line. The old line is an
   ordering comparison, not `==` or `!=`, with no macro or block, of the
   same parameter in the same direction as the new guard (an inverted old
   guard refuses). As in
   RIPR-SPEC-0227 rule 3, an old equality agrees with the new guard at the
   edge pair while differing elsewhere.
2. **One gated call.** The first parameter whose type is `&dyn T` or
   `&impl T` is the receiver. `T` must be one single-segment trait name
   without generics, so a lifetime (`&'a dyn T`), a bound list
   (`dyn T + Send`) or a qualified path refuses. The receiver is a plain
   binding (no `mut` or `ref`), named exactly once (a raw `r#` spelling
   counts) in the owner body: as the receiver of
   one method call `m` inside the guarded block. The call must be one of:
   - a whole statement;
   - a `return`;
   - a `let` initializer;
   - the block's tail.

   Nothing before the call in the block, and nothing in its arguments, can
   leave, branch, loop, short-circuit, await or hide code. A `format!`
   whose tokens cannot leave is allowed. `T` is declared once in the
   owner's file, under an attribute whose text contains `automock`, without
   generics, and it declares `m`.
3. **One counted mock.** A plain related test (the RIPR-SPEC-0227 rule 3
   test gates: one `#[test]` attribute and no `should_panic`; no `return`,
   closure, nested item, non-assertion macro or attribute in the body; a
   readable test file) meets each of these:
   - It binds `let mut x = MockT::new();` (or `::default()`), the body's
     only binding of that name.
   - It sets exactly one `x.expect_m()` chain. That chain has exactly one
     literal count, `times(N)` or `once()`, and otherwise only `with`,
     `return_const`, `return_once` and their `_st` forms. A range count,
     `in_sequence`, a closure form or a second count refuses. `never()` and
     `times(0)` are read but can never meet rule 4, which needs a taken
     input. The chain is a statement: a `let` that binds an expectation
     chain refuses, since the binding is a second name for it.
   - Every other naming of `x`, in the body or inside macro tokens and
     counting raw `r#x`, is either an `expect_*` chain on another method or
     `&x` (never `&mut x`) in the owner call's slot for that parameter. The
     mock binding has no type annotation.
   - The body holds no `if`, `match`, loop, `?`, `break`, `continue`,
     `&&`, `||`, `async` block or labeled block, and does not name
     `checkpoint`, `forget`, `leak`, `catch_unwind`, `ManuallyDrop`, `exit`
     or `abort`.
   - Every owner call runs exactly once: inside a top-level statement, a
     `let` initializer, or a checked operand of `assert!`, `assert_eq!` or
     `assert_ne!`, never a message argument.
   - Each owner call passes every argument the owner declares, and an
     integer literal for the compared parameter. The bound's `const` name
     is not read as its value on the test side: a glob import or a
     macro-defined item can rebind the name where the test reads it.
4. **Both sides, counted.** The number of owner calls whose input the
   guard takes equals `N`, so the test passes on the current code. One
   taken input and one skipped input are adjacent integers.

Moving the edge in either direction flips exactly the inputs it crosses,
all in the same direction. The adjacent pair guarantees that at least one
input flips, so the count leaves `N`. mockall panics on a call past the
expected count. When the mock drops, it also panics if the method ran
fewer times than expected.

When the rule holds, the discriminate stage reads `Yes` with high
confidence and the evidence line names RIPR-SPEC-0295. The finding then
reads `exposed`.

## Required Evidence

- Unit tests pin the corpus shape, the admitted owner and test variants,
  and the owner and test refusals, each with a premise check that the
  source still reads.
- On the Rust verdict corpus, `lib-mockall-grace-boundary-times` moves from
  `false_actionable` to `ideal`, and the runtime-labeled
  `lib-mockall-count-*` rows for examples 2 to 6 read `ideal`. No other row
  moves, and none becomes `false_exposed`.

## Non-Goals

- Mock frameworks other than mockall's `automock`, hand-written fakes and
  call counters (`AtomicUsize`, `Cell`), and `mock!` declarations.
- Owners whose gated call is reached through a helper, a loop, a closure, an
  `else` branch or a second guard. Receivers held in `self`, a `Box` or a
  generic parameter.
- Tests that count through `checkpoint`, sequences, range counts or a
  closure matcher (`withf`, `returning`), and tests whose inputs are not
  integer literals.
- Judging the returned value. The rule says nothing about what `send`
  returns or whether the result assertions discriminate on their own.
- Edits that invert the guard (an old `x <= 30` against a new `x > 30`). The
  old line must compare the same parameter in the same direction as the new
  guard, so the two test inputs bracket an edge that moved; an inverted old
  guard takes the opposite input, the count is the same under both, and the
  predicate stays `weakly_exposed`. The credit is the edge-pair claim, as in
  RIPR-SPEC-0227, not a claim about every mutant of the predicate.
- Tests in a file that declares or imports an item named like the generated
  mock (`struct MockT`, `use other::MockT`), globs in anything but
  `super::*`, or defines a `macro_rules!`: the name may not resolve to the
  `automock` type and a stand-in enforces no count, so the file refuses.

## Acceptance Examples

Each example below other than example 1 is an owner of the problem's shape,
`if overdue_days > GRACE_DAYS { return mailer.send(user, "late"); } false`,
tested only through statement calls under one mockall count.

1. The problem's owner and test: the predicate reads `exposed`.
2. `times(1)` with one call at 31: the count sees one side only, so the
   predicate stays `weakly_exposed` (`<=` survives).
3. `times(1..3)` with calls at 31 and 30: a range count refuses, so the
   predicate stays `weakly_exposed` (`<=` makes two calls, inside the
   range).
4. The owner also calls `mailer.send(user, "reminder")` after the guard,
   with `times(2)` and calls at 31 and 30: every call sends once whichever
   side the guard takes, the receiver is named twice, and the predicate
   stays `weakly_exposed` (both mutants survive).
5. `times(1)` with calls at 40 and 3: no adjacent pair, so the predicate
   stays `weakly_exposed` (both mutants survive).
6. `times(1)` with calls at 31 and 30 and no assertion: the count alone
   discriminates, so the predicate reads `exposed`.

## Test Mapping

- `crates/ripr/src/analysis/classifier/evidence/side_flip/mock_count.rs`
  tests: example 1 (`the_corpus_shape_reads_and_counts`), the admitted
  variants (example 6 among them), examples 2, 3 and 5 among the test
  refusals, example 4 among the owner refusals, the old-line edge move
  (`the_old_line_must_move_the_same_edge`) and the declared or imported
  mock name (`a_file_that_declares_or_imports_the_mock_name_is_not_counted`).
- `fixtures/rust-verdict-corpus`: example 1
  (`lib-mockall-grace-boundary-times`) and examples 2 to 6
  (`lib-mockall-count-one-side`, `-range`, `-sent-on-both-paths`,
  `-far-inputs`, `-both-sides`).

## Implementation Mapping

- `crates/ripr/src/analysis/classifier/evidence/side_flip/mock_count.rs`:
  `discrimination`, `CountGate::read`, `sole_gated_call`,
  `trait_is_automocked_with`, `CountGate::count_reading`,
  `CountGate::counted_on_both_sides`.
- `crates/ripr/src/analysis/classifier/evidence.rs`: the rule pairs its
  own oracles beside the RIPR-SPEC-0227 rule 3 guard flip.

## Metrics

- `rust_mock_call_count_mismatch`: acceptance examples whose observed
  reading differs from this spec. Target: zero.
