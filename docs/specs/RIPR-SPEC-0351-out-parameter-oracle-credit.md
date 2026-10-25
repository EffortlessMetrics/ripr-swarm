# RIPR-SPEC-0351: A test that asserts on the local it passed as `&mut` observes the changed method only when the changed expression writes through that parameter

Status: proposed

Owner: product / analysis

Created: 2026-10-23

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8999

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Rust static analysis stays advisory; this only stops one
  call shape from being reported as unobserved.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No output schema change.

## Problem

`OffsetFormat { .. }.format(&mut s, off).unwrap(); assert_eq!(s, "+09:30");`
passes a local to the changed method as `&mut` and asserts on that local. The
assertion never names the method, so lexical matching left the builder
statement as the only apparent oracle and the finding stayed
`reachable_unrevealed` even though the test reads exactly what the method
wrote.

## Behavior

An `assert_eq!` is admitted as an oracle for a direct call of the changed
method when every link is syntactic and exact:

1. The changed method has a receiver and a `&mut` parameter that its body
   uses only through the allowlist in Known Limits (top-level additive writes,
   a leading `clear()`, read-only calls) and never rebinds or shadows.
2. One operand of the assertion is a bare local. The other operand does not
   mention it.
3. In the same test body, that local is bound exactly once by
   `let mut local = <recognized empty constructor>;` (`String::new()`,
   `String::default()`, `Vec::new()`, `Vec::default()`, `VecDeque::new()`,
   `VecDeque::default()`; no `let ... else`), and no statement between that binding and
   the call mentions it.
4. The next mention is a top-level statement that calls the method, possibly
   through `?`, `.unwrap()` or `.expect(..)`, passing exactly `&mut local` at
   an out-parameter position, with the local named once in the statement.
5. The call and the assertion are top-level statements of the test, in that
   order, and the call's receiver resolves to the owner's impl type by the
   existing relation-layer receiver rules. No new receiver type inference is
   added.
6. When the changed expression is itself an unconditional additive write
   through that parameter (the same allowlist as item 1), the assertion also
   confirms observation;
   otherwise it is only a candidate oracle.

Anything else (a rebinding or shadow, a second pass of the local, a use
inside a loop, closure or macro, an assertion before the call, a call in
another test, a different or longer-named local, a `should_panic` test, an
unresolved receiver) stays uncredited. The helper decides only that the
assertion reads the call's out-parameter; it grants no strength or reach.

## Required Evidence

- Positive: asserting the local passed `&mut` exposes the changed line.
- Negatives: another local, a whole-word near miss, a non-local assertion
  operand, an assertion before the call, a rebinding, an intervening write,
  a second pass and a call in another test are each not credited.

## Known Limits

- Credit needs a clearly non-empty expected operand: a non-empty string or
  byte-string literal, a non-zero integer, such a literal with
  `.to_string()`/`.to_owned()`/`.into()`/`.to_vec()`, `String::from(lit)`, or
  a non-empty `vec![..]`. Every empty spelling, local and other expression is
  refused, as is an operand equal to the local's initializer. Like every
  exact-value oracle, the helper decides that a discriminator exists, not
  that the test passes.
- The owner is accepted by an allowlist. Every use of the out-parameter must
  be a top-level write statement, a `clear()` before the first write, or a
  read-only call (`len`, `is_empty`, ...) on the bare identifier. Later
  additive writes are allowed because they cannot erase the changed write;
  every other later use (passing it onward, reborrowing, aliasing, deref or
  parenthesized receivers, closure capture, `clone_from`, `split_off`,
  `truncate`, `drain`, any other macro) refuses.
- A write counts only when its arguments certainly add data: `push`,
  `push_back`, `push_front`, `write_char` and two-argument `insert` always;
  `push_str`, `write_str`, `insert_str` and `write_all` with a non-empty
  literal, `&format!(..)` whose template has non-placeholder text or an
  explicit width;
  `extend`/`extend_from_slice` with a non-empty array literal or
  `"lit".chars()`/`.bytes()`;
  `write!` with such a template; `writeln!`. `append`, capacity-only
  operations (`reserve`, `shrink_to_fit`), empty literals, `format!("")`,
  `None` and empty constructors are not writes. A format template containing
  `$` or `*` is never certainly non-empty, and a string literal with a
  backslash-newline or an escape other than `\n \t \\ \" \'` is not a
  non-empty literal; raw literals are plain.
- Identifier-valued text and collection arguments, including references to
  empty `String`/`Vec` locals, do not establish that a writer adds data and
  are refused (#10058). The helper has no local value model, so named
  nonempty values also refuse. This does not resolve data-dependent
  `format!`/`write!` arguments or width padding on a user `Display`.
- Writes must be straight-line top-level statements with no `return`,
  `break`, `continue` or non-trailing `?` before the last one, and the changed
  expression must itself be such a write with no branch, loop, closure or
  early exit (tokens `if`, `match`, `for`, `while`, `loop`, `|`, `?`,
  `return`, `break`). A changed token that selects a side without those
  tokens (`then_some`, indexing, `get`, width or precision, `abs()`) is not
  detected; see follow-up #10058.
- Every statement at or before the last write may contain none of `return`,
  `break`, `continue`, `loop`, `while`, `for`, `if`, `match`, `else`,
  closures, `async`/`await`, `exit`/`abort`/`panic_any`/`resume_unwind`, in
  code or macro arguments; macros must be on the pure std list and not
  redefined by the workspace; a call to a bare identifier (a possible alias
  of `exit`) refuses; and so does a call whose callee is not an identifier or
  path (turbofish `f::<T>()`, `(f)()`, `f()()`), in the owner, in test setup
  and assertions, and in the owner call statement's receiver and arguments
  (only the owner method, `unwrap` and `expect` are extra callees there).
- Any attribute in the owner body refuses. The owner fn, its impl and every
  enclosing module may carry only inert attributes (`doc`, `inline`,
  `must_use`, `allow`, `track_caller`, `cold`, `deprecated`), and the file may
  hold exactly one method of that name in impls of the owner's type. Twins in
  other files are not seen.
- A user type, alias or `use` named `String`, `Vec` or `VecDeque` in the
  owner's or the test's file refuses; a std-rooted import does not. Any
  non-std glob refuses except `use self::*` and `use super::*` inside an
  inline module (a top-level `use super::*`, `use crate::*` or `use other::*`
  can bring a user `String` from another file).
- `write_str`, `write_char`, `write_all`, `write!` and `writeln!` are
  credited only when the owner's file imports a std/core/alloc `Write` (or a
  std prelude glob) and no workspace file implements a non-std trait for
  `String`/`Vec`/`VecDeque` or declares a trait method named `write_str`,
  `write_char`, `write_all`, `write_fmt` or `write`. `write!`/`writeln!` also
  need the bare parameter as the whole first argument (`write!(out.clone(),
  ..)` refuses), resolve by exact std path, and refuse when a workspace file
  defines a macro whose name starts with `write`.
- Between the test's start and its assertion, every statement other than the
  setup definition and the owner call must be a `let` of a plain value or an
  assertion macro. In those intervening statements (and the owner call
  statement), the only calls allowed, including inside assertion, `format!`
  and `vec!` macros, are pure `len`/`is_empty`/`as_str`/`to_string` on the
  local, `String::from`/`String::new`/`Vec::new`/`VecDeque::new`, and
  `Some`/`Ok`/`Err`. The asserting statement's expected operand is checked
  separately by the non-empty expected-value rule above.
- The parameter type must be a concrete `&mut String`, `&mut Vec<..>` or
  `&mut VecDeque<..>`; generic, `impl Trait` and `dyn Trait` outputs refuse.
- Only `assert_eq!` and a bare local operand are recognised. Out-parameters
  reached through fields, helper wrappers or returned references are not
  followed.
- Named limits not followed (#10058): workspace functions that exit or panic
  (path, method and UFCS calls, and diverging user `Drop`, operator, `Index`
  and `Deref` impls, owner and test side); a helper hidden in the owner call's
  receiver or arguments beyond the scan above; macros from other crates;
  traits from external crates; `#[cfg(test)]`/`#[cfg(not(test))]` callee
  twins; data-dependent formatted empties and width padding on a user
  `Display`; a std `Write` import counting for the whole file scope. `write!(out.clone(), "+")`
  with expected `"+"` can still be credited by a pre-existing literal-match
  route outside this helper; that is not a false promotion (the native test
  fails on the original) and is unverified against main.

## Non-Goals

- Runtime mutation proof or sufficiency claims.
- Receiver type inference beyond the existing relation-layer rules.

## Acceptance Examples

1. A test builds `let mut s = String::new();`, calls
   `OffsetFormat.format(&mut s, 570);` and asserts `assert_eq!(s, "+570");`;
   the changed push line is `exposed`.
2. The same test asserting a different local, `s2`, `s.len()`, or asserting
   before the call is not `exposed`.
3. A rebinding of `s` after the call, a write to `s` before the call, or a
   second pass of `&mut s` is not `exposed`.

## Test Mapping

- `crates/ripr/tests/out_parameter_oracle.rs`.
- `fixtures/out_parameter_unrelated_write_no_promote` and
  `fixtures/out_parameter_empty_expected_no_promote`, plus
  `fixtures/out_parameter_named_empty_no_promote`, pinned as
  RIPR-SPEC-0108 non-promotion cases in
  `fixtures/evidence-promotion-honesty-corpus/corpus.json`.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/out_parameter.rs`:
  `assertion_out_parameter`.
- `crates/ripr/src/analysis/classify/reveal.rs`: `analyze_related_assertions`
  consumes it through `ReturnOracleAdmission::out_parameter_read`.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
