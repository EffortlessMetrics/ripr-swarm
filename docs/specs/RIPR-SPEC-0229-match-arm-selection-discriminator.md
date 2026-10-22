# RIPR-SPEC-0229: Match-arm selection names the unselected arm

Status: accepted

Owner: product / analysis

Created: 2026-10-04

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #5432 (a proposed unknown-not-a-gap rule 3 withheld the canonical
  sibling-arm gap; that rule was dropped, #6666)

Linked PRs:

- #5416 (unknown, not a gap, RIPR-SPEC-0240; its proposed rule 3
  `rust_oracle_target_unresolved` was dropped, #6666)
- #5638 (first implementation; see Implementation Status)

Support-tier impact:

- No tier change. A Rust `match_arm` finding whose related tests provably
  select a different arm keeps its gap and names the arm that no test selects.
  A finding whose related test provably selects the changed arm and pins the
  result exactly can read `exposed`. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. The missing discriminator reuses the existing
  `MissingDiscriminatorFact` (`value`, `reason`) and the existing
  `Missing discriminator value:` entry in `Finding.missing`.

## Problem

ripr never decides which arm a test input selects. For a changed `match_arm`
probe, reveal confirms only when an identifier after `::` in the probe text
(the arm pattern, plus the body on the lexical path) appears somewhere in an assertion, or when a string-literal arm equals a
direct owner-call argument (`analysis/classify/reveal.rs`,
`match_arm_variant_tokens` and `owner_call_literals`). Activation produces no
missing discriminator for the `match_arm` family
(`analysis/classify/activation.rs`, `missing_discriminator_facts` handles
predicate, error path and field construction only). Observed owner arguments
lose their wrapper, so `reason(Some(5))` is recorded as `x = 5`.

Three wrong outcomes follow:

1. **Correct gaps abstain.** `fixtures/match_arm_blind` has one test,
   `assert_eq!(reason(Some(5)), 6)`, and an added `None => 0` arm. No test
   selects `None`. With no named missing discriminator the gap has no
   stated cause, and a rule that withholds a strong nearest oracle without one
   (#5416's proposed rule 3, `rust_oracle_target_unresolved`, dropped for this
   reason in #6666) reads the real gap as an analyzer limit.
   #5432 counts 24 findings across the `match_arm_*`,
   `error_path_diagnostic_*` and `owner_return_pin_identity_traps` fixtures
   on the same route.
2. **Selected arms never credit.** `assert_eq!(reason(None), 0)` pins the
   changed arm exactly and still reads `weakly_exposed`, because `None` has no
   `::` token. Option and Result constructors, integer, char and bool
   literals, ranges, bindings, tuples and `_` never confirm, whatever the
   input. RIPR-SPEC-0093 records this as an accepted failure mode.
3. **An expected-side token confirms a sibling arm.** For
   `match k { Kind::A => Kind::B, Kind::B => Kind::A }` and the test
   `assert_eq!(f(Kind::A), Kind::B)`, a change to the `Kind::B =>` arm is
   confirmed by the `B` on the expected side, although the input selects the
   `Kind::A =>` arm.

## Behavior

### Selection facts

For a changed `match_arm` probe, activation collects every related test's
owner calls whose scrutinee is a parameter of the owner (or, by decision 6,
one integer `/` or `%` of it by a literal), not shadowed or reassigned
before the `match`. Each call is either resolved (its argument is
statically resolved, below) or unresolved; an unresolved call is kept as a
blocker, never dropped. A resolved call **provably selects** an arm when, in
source order,
every earlier arm provably does not match the argument and that arm provably
does.

Provable matching is defined for this pattern grammar and no other:

- integer, char, bool and string literals;
- integer ranges with literal bounds (`a..=b`, `a..b`, `..=b`, `a..`);
- enum variant paths, qualified (`Kind::A`) or bare imported, including the
  prelude constructors `Some`, `None`, `Ok` and `Err`;
- tuple-variant and tuple patterns whose subpatterns are in this grammar,
  wildcards (`_`) or bare bindings;
- or-patterns whose alternatives are all in this grammar;
- `name @ subpattern`, which matches exactly when `subpattern` does;
- a top-level `_` or bare identifier binding, which matches every argument.
  A binding with an `@` subpattern is not a catch-all.

An arm with a guard (`pat if cond`) is not provably matched or provably
unmatched. When a guarded arm comes before or is the changed arm, the call
does not provably select any arm at or after the guard.

A resolved argument is a literal, an enum path or a constructor call of the
grammar above written in the test call itself, or an immutable `let` binding
in the same test whose initializer is one and which is not shadowed before
the call. A `let mut` binding, or one passed as `&mut` before the call, is
unresolved. Arguments from helpers, fixtures, loops,
parameters or other calls are unresolved, except the helper-lent literals
of decision 5 and the constant-row table cells of decision 7, which only
credit the selected arm.

### Naming the unselected arm

When every related test call to the owner is resolved and none provably
selects the changed arm, activation emits one missing discriminator:

- `value`: the changed arm's pattern text, without the guard and `=>`
  (for example `None`).
- `reason`: names the arm and the observed values, for example
  "No related test call selects arm `None =>`; observed `x` values:
  `Some(5)`."

Observed values keep their constructor (`Some(5)`, not `5`).

The finding keeps its gap (`weakly_exposed` when a related test holds an
oracle on the result, `reachable_unrevealed` when none does), now with a
named cause.

When the change is to the arm's pattern rather than its body, the arm is
named only when no resolved call selects it under either the original or
the changed pattern: an input that moved between arms is a discriminator.

When any related call is unresolved, or a guard blocks the decision, no
missing discriminator is named, and the finding keeps the class it would
have without one.

### Crediting the selected arm

When a related test call provably selects the changed arm and the same test
holds an admitted exact oracle on that call's result (the call itself as an
`assert_eq!` operand, or a `let` binding of the call that a later exact
assertion observes, as in RIPR-SPEC-0186 pairing), discrimination is
confirmed and the finding may read `exposed`. Selection replaces token
matching as the confirmation for that test.

This holds for a change to the arm's body. For a change to the arm's
pattern, selecting the changed arm is not enough, because an input inside
both the original and the changed pattern runs the same arm either way.
Credit then needs a resolved call whose selected arm differs between the
original and the changed `match`, two arms whose results are provably
unequal values, and a same-test exact oracle on that call's result. Results
are provably unequal only when both are literals, or enum paths or
constructors of literals in the grammar above, with different values. Any
other result expression (`1 + 1` against `2`, a call, a binding) gives no
credit, because different text can compute the same value.

### Exact pattern pins through a local `assert_matches!` (#6616)

A test may pin the owner's result with a test-local macro instead of
`assert_eq!`:
`assert_matches!(parse_unit("q"), Err(UnitError::Unknown))`. Such an
assertion confirms a selected arm only when ripr can read from source that
it fails on any other value (`crates/ripr/src/analysis/syntax/pattern_pin.rs`):

- the test's file defines exactly one macro of that name, a `macro_rules!`
  with no attributes in a module enclosing the test and before it, imports
  nothing of that name, has no `macro_use` and no item-position macro call;
- the macro's first rule takes exactly `$v:expr, $p:pat` (optionally
  `$(,)?`) and the invocation is exactly one expression and one pattern, so
  that rule is the one that runs; its transcriber is `match $v { $p => {}
  <catch-all> => panic!(..) }` (or `unreachable!`), and the panic arguments
  cannot leave the test;
- the test is a plain `#[test] fn` with no parameters or return type, the
  invocation is a top-level statement, and every earlier statement is on an
  allowlist that completes or fails the test (a plain `let` or expression
  built from calls, paths, literals and operators, a standard assertion
  macro, another admitted `assert_matches!`);
- the pattern is exact (constructors, paths, literals, tuples; no binding,
  `_`, `..`, range, alternative or reference), and the value operand is a
  direct owner call whose input selects the changed arm;
- the shared owner-pin binding authority finds no visible rebinding of the
  standard macros the pin relies on (`panic`, `stringify`, earlier
  assertions).

Anything else leaves the assertion to the token rule. Any invocation
delimiter takes the same rule, so it opens no other credit path.

### Selection outranks tokens

When every related call is resolved and none selects the changed arm, a
variant token in an assertion does not confirm the arm. This closes the
expected-side confirmation in Problem item 3. When any related call is
unresolved, token matching does not confirm the arm either: only a resolved
selecting call with an exact oracle confirms, and otherwise the finding
stays below `exposed`.

### An unchanged arm body adds no finding

When only an arm's head changed (`"warning" | "warn" => Ok(Level::Warn)`
from `"warn" => Ok(Level::Warn)`), the line carries the arm finding alone.
The arm's return value lies in text the old and new lines share, so a
`return_value` finding would ask for a discriminator of a value the edit
never touched (#7074). The same holds on any line where another parser
shape holds the whole edit, such as `if 256 <= len { return Err(..) }` from
`if len >= 256 { .. }`. A return value is kept when the edit touches it, or
when no other shape on the line holds the edit, so a changed line never
loses its only finding.

### A guard-only edit is the guard's predicate

When an edit lies wholly inside one comparison of an arm's guard
(`n if 0 > n =>` from `n if n < 0 =>`), the line carries the guard's
`predicate` finding alone. A deleted or added conjunct (`n if a && 0 > n`
to `n if 0 > n`) is more than that comparison's edit, so the arm head
stays; an edit inside one operand of an unchanged `&&`/`||` chain is
still the guard predicate's.
Arm selection never evaluates guards, so an arm finding there can only
read unconfirmed, while the predicate's boundary pairing is the
discriminator the edit needs. The arm head is kept when the edit reaches
past the guard's predicate (a renamed binding, a changed pattern) or when
the line has no predicate shape that holds the edit.

A bare binding in a guarded arm (`p if 100 <= p =>`) holds the scrutinee's
value, so when the owner body's only `match` is on a parameter
(`match points {`), the guard's boundary reads that parameter's test
inputs and can name `p == 100` as missing. The operand stays unread when:

- the body has a second `match` keyword, however spelled (`match(other)`,
  or `match` ending its line), since the arm could belong to either;
- the binding's name appears anywhere but inside a one-line guard,
  between `if` and `=>`: on another line or in the arm body after `=>`
  (a `let`, a loop variable, a closure parameter), or in a guard that is
  not a plain expression (a block, a statement, a closure, a `let`, `for`
  or `match`);
- the parameter appears anywhere but the signature (which ends at its
  first `{`) and its `match` line, read with comments and string contents
  masked so neither hides code after it (`let points = points - 50;`, a closure
  `|points|`, a statement after the `{` on the signature line), since it
  may be rebound or shadowed before the `match`;
- the binding's name may resolve to a constant (a `const` or `static` of
  that name in the owner file, or an import of it), since the pattern then
  compares the constant instead of binding the scrutinee, or the owner
  file is unavailable, does not parse cleanly, or may declare items through
  a macro (`macro_rules!`, `include!`, a macro call at the item level of
  any module, or a statement macro inside the owner; a statement macro in
  another function scopes its items there and does not count);
- the binding is not a bare identifier.

The old line such an edit replaced (its positional counterpart, not any
line with the same text) is not probed again as deleted behavior: the
guard's predicate already carries the old guard as its `before`.

### Decisions

Steven delegated these choices on 2026-10-04 ("make reasonable documented
decisions and proceed"). Each records the adopted option, why, and the
rejected alternative. Any can be reversed later without touching the rest.

1. **Pattern grammar.** Adopted: the list above, including ranges and
   bare imported variants, because each is decidable from the pattern's
   syntax, the file's `use` imports and the argument literal alone. Rejected, narrower: literals and
   qualified enum paths only.
2. **`_` and binding arms by first-match.** Adopted: a trailing `_` is
   selected when every earlier arm provably does not match, so a change to
   `_ => 2` with only `f(Kind::A)` (selecting `Kind::A =>`) names `_` as
   missing. Rejected: never name `_`.
3. **Mixed resolved and unresolved calls.** Adopted: any unresolved call
   blocks the "no call selects" claim (fail closed), because the unresolved
   input may select the arm. Rejected: name the arm from the resolved calls
   and disclose the unresolved ones.
4. **Selected-arm credit.** Adopted: credit (`exposed`) as written above,
   because an input that provably selects the changed arm, paired with an
   exact oracle on that call's result, is the discriminator the change
   needs. Rejected: name the selection in the discriminate summary but keep the
   existing token rule for credit.
5. **Helper-lent literal inputs (#7981).** Adopted: a test-local check
   helper admitted under RIPR-SPEC-0197 rule 7 (#6482), whose assertion
   is `assert_eq!` with one operand exactly an owner call on a helper
   parameter, lends that parameter's literal argument to the owner call.
   When the literal at that position of a call to the helper provably
   selects the changed arm, the helper's exact assertion credits it
   (`check_color('S', "black")` with `assert_eq!(suit_color(suit), want)`).
   Only the scalar literals `loan_call_literals` reads are lent, and only
   when every argument of that helper call is one, so
   `check_color('S', want)` lends nothing; a variable, an expression or a
   call argument lends nothing. This credits
   only: a helper call still blocks naming an arm as unselected
   (decision 3). A lending test that also names the owner must pass the
   direct path's identity gates (every mention a readable call, no owner
   result bound with `let`, no local shadow of the owner), as in
   decision 7. Rejected: keep helper arguments unresolved, which reads
   a helper-wrapped table of literal checks as unproven.
6. **Integer scrutinee arithmetic and value-equal respelling (#7989).**
   Adopted: a scrutinee written `param / N` or `param % N`, with `param` a
   primitive integer parameter (`u8`..`u128`, `usize`, `i8`..`i128`,
   `isize`) and `N` a positive integer literal, binds `param`. An integer
   literal input is evaluated as Rust does (truncating division, remainder
   with the dividend's sign), so `http_class(301)` reaches the `3` arm of
   `match status / 100`; any other input is not provable. A longer
   expression, another operator, a non-literal or zero divisor, or a
   non-integer parameter establishes nothing. Separately, a changed arm
   whose alternatives are all literals or literal-bounded integer ranges,
   each matching an alternative of the original by value and vice versa
   (`3` to `0x3`; `1..=3` to `1 | 2 | 3` still reads as changed), reads as an
   unchanged pattern, like reordered alternatives. Rejected: keep every
   computed scrutinee a non-goal, which leaves the common
   status-class and weekday matches unproven.
7. **Constant-row table inputs (#7989).** Adopted: an `assert_eq!` whose
   compared operand is exactly a direct owner call fed by a column of a
   constant-row `for` table (#5328, read by `constant_table_column`) is a
   selecting call when some row's cell selects the arm, since the loop
   runs the assertion once per row. Like decision 5 this credits only;
   table rows never name an arm as unselected. The assertion must be the
   test's own `assert_eq!` statement standing directly in the body of the
   `for` that binds the column, with no `continue`, `break`, `return` or
   `?` before it in that body and no block, closure or call around it; an
   assertion borrowed from a check helper never reads the test's table.
   As on the direct path, an assertion whose every compared operand names
   the owner never selects.
   The test must still pass
   the direct path's owner-identity gates: every owner mention a readable
   call, no owner result bound with `let`, and no local shadow of the
   owner. Rejected: keep loop bindings unresolved.

### Implementation Status

#5638 implements the Behavior above for a subset of the grammar. Everything
outside the subset reads as not provable, which never names an arm and never
credits one.

Implemented:

- integer, char, bool, and cooked or raw string literals, compared by value.
  A char-literal pattern (`'C' | 'S' =>`) reads since #7981; before it the
  arm-head reader masked char literals as comments and no char arm ever
  established;
- literal arguments a check helper lends to an owner call (decision 5,
  #7981), for credit only;
- `param / N` and `param % N` integer scrutinees and value-equal literal
  respellings (decision 6, #7989), which both select and name arms;
- constant-row table cells lent to the owner call, for credit only
  (decision 7, #7989);
- integer ranges with literal bounds (`a..=b`, `a..b`, `..=b`, `a..`),
  matched by value against an integer input (#5992);
- enum variant paths, qualified or bare imported, including `Some`, `None`,
  `Ok` and `Err`, when the payload is only `_`, `..` or bare bindings. A
  variant name with no lowercase letter (`Kind::A`, `Method::GET`) is read
  only as described below;
- or-patterns of these, and a top-level `_` or bare binding;
- first-match order: a guarded earlier arm blocks selection, and an earlier
  arm that provably matches means the input selects another arm;
- a changed pattern names the arm only when neither the original nor the
  changed pattern selects any observed input. It earns selection credit
  only from an input that moved: the input runs this arm under exactly one
  of the two patterns, the first later arm that provably takes it under
  the other is found with no guarded arm in between, and the two arms'
  results are provably unequal values (#5992). The original `match` is
  read as the current one with this arm's old pattern restored, so the
  credit says the test notices this line reverting. When the diff adds or
  removes more than one arm line in the owner, the other arms may not be
  the original ones, so the diff keeps the whole old line instead of a
  head cut (guarded arms excepted) and the moved input earns no credit.
  The credit needs the diff's old
  text to be a head cut anchored on this arm's unchanged body. A whole
  old line never earns it, even with an equal body, and an uncut old head
  arrives joined to the next line of the old file, so it never passes for
  a cut. It also needs
  the `match` to be the owner body's tail expression, so an arm's result
  is the return value. Results compare as literals of one kind, or as
  variants under the same qualifier by name, so `Kind::Alpha` and an
  imported `Alpha` never read as unequal. Only `Some`, `Ok` and `Err`
  compare by literal payload: another CamelCase path with arguments may be
  a function call (`Make(1)`) whose value need not keep its arguments.
  A pattern whose alternatives were only reordered (`"CA" | "MX"` to
  `"MX" | "CA"`) takes the same inputs and reads as unchanged.
  A changed arm's original is the adjacent removed arm with the same
  pattern, else the first adjacent removed line sharing a token. A
  qualified enum name is such a token for every arm of a multi-line hunk,
  so a token-paired original counts as read only when it shares an
  alternative with the changed pattern, or is a head cut anchored on this
  line's unchanged body (`1..=8 =>` for `1..=9 => "digit",`). A token-paired original that
  shares one may still be another arm of the hunk; the arm is then named
  against that arm's pattern;
- selection outranks tokens whenever the scrutinee is a direct owner input.
  A test that never names the owner (it reaches it only through a wrapper)
  passes no input to read, so its assertion tokens confirm as they did
  before selection (#6297, `match_arm_proximity_wrapper_confirms`). An
  owner imported under an alias in the test's file, or a file whose imports
  were not read, counts as naming it. A wrapper-only test whose expected
  side names a sibling variant therefore still confirms, as on main.

Not yet implemented (each reads as not provable):

- char ranges, path-bounded ranges, tuple patterns, refutable payload
  subpatterns and `@`;
- arguments resolved through an immutable `let` in the test;
- credit through a `let` binding of the call (RIPR-SPEC-0186 pairing).

The implementation adds these fail-closed conditions, which the Behavior
implies but does not spell out:

- a `self` receiver is a scrutinee like a parameter, when it cannot change
  before the `match` (a `mut self`, `&mut self`, `self: &mut Self` or
  `self: Pin<&mut Self>` receiver used anywhere else refuses);
- a free owner called through a path (`a::reason(..)`) is read only when
  the path starts with `crate`, `self`, `super`, `Self`, the owner's impl
  type, or the indexed root package or its lib target (a `pub use`
  re-export under that root is not resolved);
- selection credit requires the `match` to be the owner body's first
  unconditional expression, since a nested, short-circuited or
  early-returned match may never run;
- a test that may reach the owner through a helper (at any depth) or a
  non-standard macro (however its `!` is spaced) has an unresolved call;
- a test whose file imports a foreign same-named function, or whose package
  defines one, has an unresolved call;
- a qualified input whose type is neither `Self` nor the scrutinee's type
  is unresolved, as is one whose path above the type starts outside the
  workspace roots (`other_crate::RenameRule::LowerCase`). An owner name
  that is not unique in a complete workspace establishes nothing;
- a bare variant input (`LowerCase`) is unresolved when the test's file
  imports that name from a path whose type segment is not the scrutinee's
  type, renames an import to it, or glob-imports another type's variants
  (`use other::OtherRule::*`). A bare or type-qualified input
  (`RenameRule::LowerCase`) is also unresolved when the file imports its
  leading name from outside the workspace roots
  (`use other_crate::RenameRule;`). A module glob in a test file
  (`use other_crate::prelude::*`) is not read and stays a known gap;
- a qualified name with no lowercase letter (`Kind::A`, `Method::GET`) may
  be an associated constant, which a pattern compares by value. It reads as
  a variant, in the changed, earlier and later arms alike (#7670), only
  when its path is `Type::NAME`, with `Type` the
  scrutinee's type written unqualified (in the parameter's signature, or
  the impl header for `self`), or `Self` in that type's own impl. The
  owner's file must declare exactly one enum of that name listing `NAME`
  and no struct, union, trait or type alias of that name, import nothing by
  that name, glob-import no module (a `use super::*` inside an inline
  module, such as `mod tests`, brings in only that file's items) and call
  no macro at file or module item level, nor any macro in statement
  position inside a function named like the owner (its expansion may
  declare a local `Kind` scoped to the match), and no function named like
  the owner may be nested in a block. Alternatively, the owner's file
  may bind that name through exactly one import, a top-level `use` without
  a rename, whose module path (`kinds::Kind` after `mod kinds;`,
  `crate::a::Kind`, `self::`, `super::`) follows recorded out-of-line `mod`
  declarations to exactly one workspace file, and that file must meet the
  same declaration rule. The owner's file must then declare no item of
  that name and call no such macro, glob-import no module inside an
  inline module (except `super::*`), and its recorded module chain must
  start at the workspace's `src/lib.rs` or `src/main.rs` or a
  `crates/<name>/src/` one. That root must be its package manifest's
  default target root (`autolib` and `autobins` on, no target moved by a
  `path` other than a `[[bin]]` naming `src/main.rs`). No workspace file
  may declare a `mod` of that file's stem (a package-root `lib.rs` with
  `mod src { mod lib; }`), use the `mod` keyword other than to start a
  plain `mod name;` or `mod name {` (macro input such as `mod $name;` or
  `m!(mod, lib)`), call or name `include!`, or carry a `#[path]`, which
  reaches anywhere; the sibling root (`src/main.rs` beside `src/lib.rs`)
  may not mention the stem at all, call an item macro, or carry an
  attribute other than a compiler builtin (a derive only of the standard
  derivable traits, and no `cfg_attr`), or import a glob or a name that
  may shadow a builtin attribute or derive, since an unread macro or proc macro there may declare
  the root a module; and no package file may sit outside
  `src/`, `tests/`, `benches/`, `examples/` and `build.rs`. Each path step
  must be the one `mod` of that name in its parent file, declared out of
  line at the file's top level, and its file may carry no inner
  `#![cfg]` or `#![cfg_attr]`. An inline or
  `cfg`-duplicated module on the path, a `cfg` or `cfg_attr` on the import
  or a module step, a chain below an inline module, a
  re-export, a rename, an `include!` or an unresolved module edge reads
  nothing. A
  test's uncased input is read
  only when no related test file outside the owner's file declares an item
  of that name, renames an import to it, or glob-imports a module other
  than `super`, a workspace root or the type's own variants, since a
  test-side `struct Kind` with `const A` makes `Kind::A` a constant.
  Otherwise, and for a bare uncased name (`A`), the pattern or input is not
  provable;
- a bare lowercase pattern (`target =>`) is a binding only when the owner's
  file declares no `const` or `static` of that name, imports no item by
  that name, and has no glob import of a module, since Rust compares a
  pattern that names a constant by value;
- only an `assert_eq!` operand confirms a selected arm: any `assert_ne!`
  form (`debug_assert_ne!`, `prop_assert_ne!`) passes for many arm results
  (`assert_ne!(reason(None), 2)` holds whether the arm yields 0 or 1);
- an `assert_eq!` whose every operand names the owner
  (`assert_eq!(reason(None), (reason(None)))`) never confirms, nor does
  any assertion in a test that binds an owner call's result with `let`,
  since that value may stand on the expected side and move with the arm.
  The module-glob rule for bindings above is broad: a `use super::*` in an
  inline test module of the owner's file demotes every binding arm there.

## Required Evidence

- `fixtures/match_arm_blind` reads `weakly_exposed` with a missing
  discriminator whose `value` is `None`, on main and with RIPR-SPEC-0240
  applied (0240 does not touch `weakly_exposed`).
- A selected-arm control (`assert_eq!(reason(None), 0)`) reads `exposed`.
- An unresolved-input control reads no named missing discriminator and is
  not upgraded.
- A guarded-arm control reads no arm-selection missing discriminator. A
  guard-only edit carries only its guard's predicate finding, which may name
  the boundary under RIPR-SPEC-0186.
- The expected-side trap reads below `exposed`.
- Golden drift on the #5432 list is reviewed row by row: each finding that
  leaves `static_unknown` names its unselected arm, and none gains `exposed`
  without a selecting call and an exact oracle in one test.

## Non-Goals

- No guard evaluation.
- Except the bounded direct-record field input described below, no scrutinee
  that is not a direct owner parameter (fields, method results,
  computed tuples), except decision 6's single integer `/` or `%` by a
  literal.
- No exhaustiveness checking beyond source-order first match.
- No change to RIPR-SPEC-0164 helper transfer, which keeps its own refusals.
- No new probe family, class, `static_limit_kind` or schema field.

## Acceptance Examples

Source for 1 to 3:
`fn reason(x: Option<i32>) -> i32 { match x { Some(v) => v + 1, None => 0 } }`,
the diff changes `None => 1` to `None => 0`.

1. Test `assert_eq!(reason(Some(5)), 6)`: `weakly_exposed`; missing
   discriminator `value` `None`, reason names arm `None =>` and observed
   `Some(5)`.
2. Test `assert_eq!(reason(None), 0)`: `exposed`.
3. Test `let x = make(); assert_eq!(reason(x), 6)`: no named missing
   discriminator; stays below `exposed`.
4. `match k { Kind::A => 1, _ => 2 }`, the diff changes `_ => 0` to `_ => 2`, test
   `assert_eq!(f(Kind::A), 1)`: `weakly_exposed`; missing discriminator
   `value` `_`.
5. `match x { Some(v) if v > 4 => 1, _ => 0 }`, the diff changes the guard
   from `v > 3`, test `assert_eq!(f(Some(5)), 1)`: no arm-selection missing
   discriminator. The guard-only edit carries only the predicate finding
   `v > 4`, which stays `weakly_exposed` and names `v == 4` as its boundary.
6. `match k { Kind::A => Kind::B, Kind::B => Kind::A }`, arm `Kind::B =>`
   changed, test `assert_eq!(f(Kind::A), Kind::B)`: `weakly_exposed`; missing
   discriminator `value` `Kind::B`.
7. `match n { 0 => "zero", 1..=9 => "digit", _ => "many" }`, arm
   `1..=9 =>` changed, tests `f(0)` and `f(42)` with exact assertions:
   `weakly_exposed`; missing discriminator `value` `1..=9`.
8. Same `match`, the diff changes the pattern `1..=8` to `1..=9`. Test
   `assert_eq!(f(5), "digit")`: not `exposed` (5 selects the arm under both
   patterns) and no missing discriminator is named. Naming the
   boundary input (`9`) as missing, as RIPR-SPEC-0186 does for predicates,
   is a follow-up.
   Test `assert_eq!(f(9), "digit")`: `exposed` (9 moved from `_` to the
   changed arm).
9. `match n { 0..=4 => 1 + 1, _ => 2 }`, the diff changes the pattern
   `0..=3` to `0..=4`. Test `assert_eq!(f(4), 2)`: not `exposed` (4 moved
   arms, but both arms return 2).

10. `fn parse_unit(raw: &str) -> Result<u32, E> { match raw { "" =>
    Err(E::Blank), "k" => Ok(1_000), _ => Err(E::Unknown) } }`, the diff
    changes the `_` arm's body. Test
    `assert_matches!(parse_unit("q"), Err(E::Unknown))` through a local
    macro whose first rule panics on a mismatch: `exposed`. The same test
    with input `"k"` or `""`, pattern `Err(_)`, the invocation after a
    `return;` or inside a closure, or a local macro that does not panic:
    not `exposed`.

## Test Mapping

- `crates/ripr/src/analysis/syntax/pattern_pin/tests.rs` and the
  `arm_selection.rs` pattern-pin unit tests (#6616): the verified macro,
  per-rule trust, delimiters, broad patterns, skipped invocations and
  rebindings. Verdict corpus cases `old-unit-assert-matches-macro` and
  `old-unit-assert-matches-sibling-arm` (negative control).

- `fixtures/match_arm_blind` (example 1, with the arm added rather than
  changed; the expected result is the same)
- `fixtures/match_arm_selected_credit` (example 2, with the arm added)
- `fixtures/match_arm_expected_side_trap` (example 6)
- `crates/ripr/src/analysis/classify/arm_selection.rs` unit tests: grammar
  elements, first-match order (examples 4 and 5), changed patterns, unread
  inputs, matches the call may skip, mutable typed receivers and foreign
  call paths.
- `crates/ripr/tests/match_arm_uncased_variant.rs`: examples 4 and 6 with
  single-letter variants; the enum imported from its module file or
  through `super::`; a struct with constants, a renamed import or a
  re-export in its place; and an owner item shadowing the import, a
  `cfg`-alternative inline module, a `cfg`-gated import beside a glob, a
  `crate::` path from below an inline module or from a module file named
  `src/main.rs` below inline modules or a package-root library, a
  nested module step beside a renamed import, a moved library root that
  declares `mod lib` through a macro (`mod $name;` or a passed `mod`
  token) or an included file, a module file compiled out by an inner
  `#![cfg]`, a sibling root whose unread macro, proc-macro attribute or
  custom derive may declare the owner a module, and a statement macro in
  the owner, including one shadowing
  `assert!`, that may declare a local `Kind`.
- `crates/ripr/tests/match_arm_unselected_identity.rs`: owner identity
  (foreign imports, a foreign call path, a shadowing closure, a two-level
  helper).
- `crates/ripr/tests/match_arm_helper_selection.rs` (decision 5): a
  helper-lent char literal that selects the changed arm credits it; a
  literal for another arm and a variable argument do not. Verdict corpus
  case `grid-match-helper`.
  A same-file `macro_rules!` that may define an item of the owner's name
  (#7985) makes the owner call ambiguous, so it credits nothing; a macro
  defining an unrelated name does not. Unit tests: `macro_item_names.rs`
  `tests` and the `helper_transfer.rs` unit test
  `test_call_is_shadowed_by_a_macro_defined_same_named_fn`, plus the shared-gate
  test `macro_defined_same_name_is_a_foreign_rebind_for_every_gate_caller` in
  `reveal.rs` (covering the owner-pin callers).
- `crates/ripr/tests/match_arm_table_selection.rs` and the
  `arm_selection.rs` unit tests
  `integer_scrutinee_arithmetic_evaluates_the_input`,
  `a_value_equal_literal_respelling_reads_as_unchanged` and
  `a_constant_row_table_lends_its_cells_to_the_owner_call` (decisions 6
  and 7). Verdict corpus case `grid-match-table`.
- `crates/ripr/src/analysis/probes/diff.rs` unit tests: an arm-head edit
  drops the unchanged return value; a body edit, or a line with no other
  shape holding the edit, keeps it. Verdict corpus case
  `config-log-level-warn`.
- `crates/ripr/src/analysis/classify/arm_selection.rs` unit tests for
  ranges and moved-input credit (examples 7 to 9):
  `integer_range_arms_select_by_value`,
  `an_input_that_moved_to_an_unequal_result_credits_a_changed_pattern`,
  `moved_input_credit_needs_provably_unequal_results`; and
  `crates/ripr/src/analysis/probes/diff.rs`
  `an_uncut_old_arm_head_carries_its_old_body`.
- Verdict corpus cases `spec0229-unselected-range-arm` (example 7),
  `spec0229-range-widened-inside-input` and
  `spec0229-range-widened-moved-input` (example 8).
- Planned: a fixture for example 3 once `let` arguments are implemented.

## Implementation Mapping

- `crates/ripr/src/analysis/syntax/pattern_pin.rs` and
  `OwnerPinSyntax::exact_pattern_pin_value`: exact pattern pins through a
  verified local `assert_matches!`, parsed once per file per run.

- `crates/ripr/src/analysis/classify/arm_selection.rs`: pattern and input
  reading, scrutinee binding and first-match selection; integer scrutinee
  arithmetic and value-equal respelling (decision 6); helper loans and
  table rows (`with_helper_loans`, `with_table_rows`, decisions 5 and 7),
  fed from `crates/ripr/src/analysis/classifier/evidence.rs` and reading
  table columns through `analysis/syntax` `constant_table_column`.
- `crates/ripr/src/analysis/classify/activation.rs`: selection facts and the
  `match_arm` missing discriminator; keep constructors on observed values.
- `crates/ripr/src/analysis/classify/infection.rs`: an unselected arm reads
  weak infection.
- `crates/ripr/src/analysis/classifier/evidence.rs`: selector gating and the
  owner-identity defeats for a named arm.
- `crates/ripr/src/analysis/classify/reveal.rs`: selection confirmation and
  the selection-outranks-tokens gate.
- `crates/ripr/src/analysis/probes/diff.rs`: a changed arm pairs with the
  removed arm of the same pattern; a return value the edit left untouched
  is dropped when another shape on the line holds the edit.
- `crates/ripr/src/domain/probe.rs` (`input_boundary_fact`) and
  `crates/ripr/src/output/related_test_miss.rs`: an examined test of a named
  arm misses an input (`missing_input`, "no test input selects arm"), not an
  exact assertion (RIPR-SPEC-0224).

## Metrics

- `match_arm_unselected_arm_named`: a `match_arm` finding whose resolved
  related calls all select other arms names the changed arm as missing.
- `match_arm_selected_arm_credit`: a resolved selecting call with a same-test
  exact oracle confirms the arm.
- Verdict-corpus false-actionable and false-silent counts on `match_arm`
  cases do not rise.

### Plain boolean assertion execution admission (#7788)

A MatchArm oracle spelled as an outer bare `assert!` must pass the existing
parser-backed assertion execution admission independently of token overlap.
A local macro with that spelling that merely evaluates its operand cannot lend
observation or a strong oracle. Assertions inside a skipped branch or an uncalled
closure are also refused. Direct standard assertions and supported eagerly called
local check helpers retain their credit. Admission filters assertion facts while
preserving the original test summary and assertion cardinality.

The public API controls in `crates/ripr/tests/match_arm_assert_admission.rs` bind a
changed unknown-option parser arm to out-of-line tests. The adversarial fixture
`fixtures/match_arm_shadowed_assert` additionally pins this false-promotion in
the evidence-promotion honesty corpus. Qualified assertion macros, fallible
`ensure!`, dependency identity and helper-result propagation remain outside this
bounded change.

## Named field-selector boundary (#8479)

A plain parameter field in a statement-free tail match is outside the credited
whole-parameter selector rules. A candidate-current weak finding with reach
`yes`, discrimination `weak`, and a strong oracle in a directly calling test
may name `rust_field_match_selector_unresolved`. The whole match header and
unique direct-arm start lines qualify. Guarded, nested, computed, local or
ambiguous same-line shapes do not. This metadata preserves every class, proof
leg and missing discriminator; it proves neither Option identity nor that a
particular invocation feeds the assertion. Guidance first asks to inspect that
alignment and only then repair a demonstrated observation gap. Existing typed
limitations, including FFI/binding exposure, take precedence. Cached classified findings must be invalidated;
file facts and output wire versions do not change.

The preserved identities are finding/probe IDs; preserved counts are raw class
counts. Existing #5268 rules withhold the canonical repair-gap projection when a
typed limitation is present. MCP repair readiness and derived gate routes may
therefore refuse these rows as analyzer uncertainty. This is not exposure,
resolved test debt, or evidence that a consumer's strict qualification passed.
The supported direct-parameter genuine-gap case retains its canonical identity
and missing discriminator.

## Direct record field inputs (#5500 / #5502)

The canonical match-arm selector also reads a bounded direct record input:

```rust
struct Lease { elapsed: Option<u128>, timeout: u64 }
fn charge(lease: &Lease) -> u64 {
    match lease.elapsed {
        Some(ms) => ms.div_ceil(1000) as u64,
        None => lease.timeout,
    }
}
// A direct argument supplies the selected field on this exact call.
assert_eq!(charge(&Lease { elapsed: Some(1001), timeout: 600 }), 2);
```

This is an extension of the existing parameter-to-arm and same-assertion
owners, not a second alias resolver. The match must remain a statement-free,
unguarded tail match on one named field of a plain immutable parameter. The
record must be a unique nongeneric root struct in the same indexed source file
as every related test; its selected field must have unambiguous prelude
`Option<u64>` or `Option<u128>` type. Namespace ambiguity, unknown item macros,
conditional fields, imported/aliased records and unread source refuse proof.
Critical raw identifier bindings retain their normalized identity for shadow
checks; raw references with unresolved provenance cannot borrow prelude names.
The current root owner's exact body must match the indexed source. Current and
original arm patterns must use unqualified prelude `Some`, `None` or `_` with
irrefutable payload bindings. A qualified constant named `None` cannot borrow
the standard variant's identity; unsupported patterns retain unresolved
selection rather than borrowing constructor spelling.
The direct shared-borrowed record literal must explicitly initialize its unique
selected field with `None` or `Some` of a literal. Other fields must also be
closed literals or constructors; spreads, shorthand, computations, mutable
borrows and a different record type refuse proof.

The existing selector consumes that initializer with its constructor intact.
Every relevant readable call selecting a sibling can name the missing arm;
an unresolved relevant call still prevents that claim. Existing assertion
admission and exact same-call/result pairing retain authority over exposure.
A selected call elsewhere in the test, or a selector-shaped value in another
field, the expected operand or diagnostics, cannot replace that pairing.

A local variable, helper-returned record and later field writes remain outside
this slice, including the current ub-review #1503 lease fixture. They require
constructor-preserving value/binding provenance from the existing authority;
this extension makes no claim that their observations or the consumer's strict
qualification have passed. The #8479 typed limitation continues to describe
those unsupported cases. Public wire schemas and gap policy do not change;
classified-cache generations must invalidate older semantic results.

The direct-record field slice also refuses unread statement macro expansions:
they can introduce caller-visible `Some`/`None` bindings without spelling those
names at the invocation. Existing macro-position and value-binding authority
admit only unshadowed trusted standard statement macros in this bounded path.
`fixtures/field_match_macro_option_shadow` pins a compiled counterexample whose
unchanged test passes even after the unobserved `None` arm is made wrong.

When supported direct record inputs all select sibling arms, the existing
missing-arm fact remains actionable. The legacy field-selector limitation must
not overwrite that established input boundary or withhold its repair identity.
Unsupported helper/mutable inputs still retain their limitation unchanged.

Direct-record same-invocation credit additionally requires the existing full
assertion execution admission. A parsed equality under `if false`, or another
unestablished execution path, is not an admitted oracle. This bounded producer
consumes the shared authority rather than adding a second path evaluator.
`fixtures/field_match_unexecuted_assertion` and the native API regression pin it.
