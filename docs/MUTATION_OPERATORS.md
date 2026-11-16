# Mutation operators ripr models

If you use cargo-mutants, mutmut or Stryker, you think in mutation operators:
"flip `<` to `<=`", "delete this match arm", "replace the body with `false`".
ripr does not generate mutants. It reads your diff and asks whether a test
appears to notice the change. So the useful question is: **when your diff
makes a change of a given kind, does ripr tell a test that pins it apart from
one that doesn't?**

This page answers that per operator and per language, from measured runs. It
is static evidence about ripr's own analysis, not a claim about runtime kill
rates. For that, see [Proof and limits](PUBLIC_PROOF.md).

## How it was measured

Each row is one canonical edit, written in Rust, Python and TypeScript where
the language has it. Each edit was committed on top of a base and run through
`ripr check --base HEAD~1 --format json` twice:

- **pinning test**: an exact assertion at an input where the old and new code
  differ, so the test would fail on the base (`assert_eq!(f(10), true)`);
- **reaching test**: the same call with nothing asserted about the result.

Verdicts:

- **yes**: the pinning test reads `exposed` and the reaching test does not.
  ripr models this operator. The matrix does not also run an exact assertion
  at an input where old and new agree, so "yes" does not show that ripr
  withholds `exposed` there. [#14840](https://github.com/EffortlessMetrics/ripr-swarm/issues/14840)
  tracks Rust cases where it does not.
- **partial**: ripr gives the change a specific probe, but does not reach
  `exposed` for the pinning test, or also reads the reaching test as `exposed`.
- **unknown**: only `static_unknown`, `infection_unknown` or
  `propagation_unknown`. ripr says it cannot tell. That is honest, but it is
  not help.
- **false gap**: `no_static_path` although the test calls the changed code.

Measured 2026-11-15 on main `60114de3`.
Python tests import names explicitly (`from fx.core import f`). ripr's probe
family is in parentheses.

## Matrix

| Operator (example edit) | cargo-mutants | mutmut | Stryker | Rust | Python | TypeScript |
| --- | --- | --- | --- | --- | --- | --- |
| Relational boundary (`x > 10` → `x >= 10`) | none¹ | `>` ↔ `>=` | EqualityOperator | yes (predicate) | yes (predicate) | yes (predicate) |
| Relational flip (`x < 10` → `x > 10`) | BinaryOperator | none⁶ | EqualityOperator (`<` → `>=`) | yes (return_value) | yes (predicate) | partial (predicate) |
| Equality (`==` → `!=`) | BinaryOperator | `==` ↔ `!=` | EqualityOperator | yes (predicate) | yes (return_value) | yes (return_value) |
| Logical (`&&` → `\|\|`) | BinaryOperator | `and` ↔ `or` | LogicalOperator | unknown | yes (return_value) | yes (return_value) |
| Negation removed (`!a` → `a`) | UnaryOperator | remove `not` | BooleanLiteral | unknown² | yes (return_value) | yes (return_value) |
| Membership (`in` → `not in`) | n/a | `in` ↔ `not in` | n/a | n/a | yes (return_value) | n/a |
| Arithmetic in the returned expression (`a + b` → `a - b`) | BinaryOperator | `+` ↔ `-` | ArithmeticOperator | yes (return_value) | yes (return_value) | yes (return_value) |
| Arithmetic in a local binding (`let y = a + b`) | BinaryOperator | `+` ↔ `-` | ArithmeticOperator | unknown | yes (return_value) | partial (return_value) |
| Compound assignment (`t += b` → `t -= b`) | BinaryOperator | `+=` ↔ `-=`, `+=` → `=` | AssignmentOperator | unknown | yes (predicate) | yes (return_value) |
| Update (`t++` → `t--`) | n/a | n/a | UpdateOperator | n/a | n/a | partial (predicate) |
| Unary minus removed (`-a` → `a`) | UnaryOperator | `-a` → `+a` | UnaryOperator (`-a` → `+a`) | unknown² | yes (return_value) | yes (return_value) |
| Bitwise (`a & b` → `a \| b`) | BinaryOperator | `&` ↔ `\|` | none | unknown² | yes (return_value) | yes (return_value) |
| Module constant (`LIMIT = 100` → `101`) | none | number +1 | none | unknown | false gap | false gap |
| Number literal in an expression (`x * 3` → `x * 4`) | none | number +1 | none | yes (return_value) | yes (return_value) | yes (return_value) |
| Boolean literal returned (`true` → `false`) | FnValue | `True` ↔ `False` | BooleanLiteral | yes⁸ (return_value) | yes (return_value) | yes (return_value) |
| String literal returned (`"hello"` → `""`) | FnValue | string mutation | StringLiteral | yes (return_value) | yes (return_value) | yes (return_value) |
| Default parameter value (`n=3` → `n=4`) | n/a | number +1 | none | n/a | yes (predicate) | unknown |
| Body replaced with a default value (`0`, `None`, `undefined`) | FnValue | none | BlockStatement | yes (return_value) | partial (return_value) | yes (return_value) |
| Collection emptied (`vec![1, 2]` → `vec![]`) | FnValue | none | ArrayDeclaration | yes⁸ (return_value) | yes (return_value) | yes (return_value) |
| Call statement deleted (`v.push(x + 1);` removed) | FnValue on the callee³ | none | BlockStatement³ | yes (call_deletion) | partial (side_effect) | yes (side_effect) |
| Method swapped (`min` → `max`) | none | none | MethodExpression | yes (return_value) | yes (return_value) | yes (return_value) |
| Call argument changed (`scale(x, 2)` → `scale(x, 3)`) | none | argument mutation | none | yes (return_value) | yes (return_value) | yes (return_value) |
| Condition forced true (`if x > 0` → `if true`) | none | none | ConditionalExpression | unknown | yes (predicate) | partial (predicate) |
| Ternary or `if` branch value (`else 2` → `else 3`) | none | number +1 | none | partial (predicate) | partial (predicate) | partial (predicate) |
| Guard clause deleted (`if x < 0 { return 0 }` removed) | none | none | ConditionalExpression | partial (return_value) | yes (return_value) | yes (return_value) |
| `break` → `continue` | none | `continue` → `break`, `break` → `return` | none | unknown | yes (predicate) | partial (predicate) |
| Loop bound (`0..n` → `0..=n`, `i < n` → `i <= n`) | none | none | EqualityOperator | unknown | yes (predicate) | partial (predicate) |
| Match arm deleted | MatchArm⁷ | `case` dropped | none | partial (match_arm) | yes (predicate) | yes (return_value) |
| Match arm value (`1 => 5` → `1 => 7`) | none | number +1 | none | yes (match_arm) | yes (return_value) | yes (return_value) |
| Match guard forced (`if n > 5` → `if true`) | MatchArmGuard | none | n/a | partial (match_arm) | yes (predicate) | n/a |
| Struct/object field value (`retries: 3` → `5`) | StructField⁴ | number +1 | ObjectLiteral | yes (field_construction) | yes (field_construction) | partial⁵ (field_construction) |
| Error variant (`Err(E::Empty)` → `Err(E::Long)`) | FnValue (error values) | none | none | yes (error_path) | yes (error_path) | yes (error_path) |
| Error path removed (`if x < 0 { return Err(..) }` removed) | none | none | ConditionalExpression | partial (error_path) | partial (error_path) | partial (error_path) |
| `?` replaced (`s.parse()?` → `.unwrap_or(0)`) | none | n/a | n/a | partial (error_path) | n/a | n/a |
| Optional chaining (`o.a?.b` → `o.a!.b`) | n/a | n/a | OptionalChaining | n/a | n/a | partial (return_value) |
| Regex (`/^a/` → `/a/`) | n/a | n/a | Regex | n/a | n/a | yes (return_value) |
| Nullish default (`x ?? 0` → `x ?? 1`, `x or 0`) | n/a | number +1 | LogicalOperator | n/a | yes (return_value) | yes (return_value) |

¹ cargo-mutants' BinaryOperator replaces `>` with `==` and `<` (the flip row).
It does not generate the `>=` boundary shift, which ripr does model.
² A returned unary or bitwise expression over names (`-a`, `!ready`, `a & b`)
stays unknown: ripr cannot yet check whether the test's inputs tell the two
versions apart (`f(0)` does not for `-a` → `a`), so it does not claim
`exposed`. [#14815](https://github.com/EffortlessMetrics/ripr-swarm/issues/14815).
³ Not a direct operator. cargo-mutants replaces the callee's body with `()`;
Stryker empties the whole block.
⁴ cargo-mutants only deletes fields of literals with a `..base`.
⁵ A test asserting only a sibling field also reads `exposed`
([#14814](https://github.com/EffortlessMetrics/ripr-swarm/issues/14814)).
⁶ mutmut only shifts comparison boundaries (`<` ↔ `<=`, `>` ↔ `>=`).
⁷ Only in a `match` that has a wildcard arm.
⁸ When the function's whole body is the constant on both sides of the diff
and the two constants fold to different values. A named, computed or
early-returned body on either side, or constants that fold equal (`false` →
`!true`), stays unknown
([#14852](https://github.com/EffortlessMetrics/ripr-swarm/issues/14852)).

Operator names: cargo-mutants genres from its
[mutation patterns](https://mutants.rs/mutants.html); Stryker mutator names
from StrykerJS's
[supported mutators](https://stryker-mutator.io/docs/mutation-testing-elements/supported-mutators/). mutmut publishes no operator table, so its column is read from
the mutmut 3.3.1 source (`mutmut/node_mutation.py`).

## Totals

| | yes | partial | unknown | false gap | rows |
| --- | --- | --- | --- | --- | --- |
| Rust | 15 | 6 | 10 | 0 | 31 |
| Python | 28 | 4 | 0 | 1 | 33 |
| TypeScript | 22 | 10 | 1 | 1 | 34 |

## Where ripr falls short, by language

**Rust.** Arithmetic in a local binding and local compound assignment read
`static_unknown`. These are among cargo-mutants' most common mutants
([#6485](https://github.com/EffortlessMetrics/ripr-swarm/issues/6485),
[#10412](https://github.com/EffortlessMetrics/ripr-swarm/issues/10412),
[#12770](https://github.com/EffortlessMetrics/ripr-swarm/issues/12770)).
Loop range bounds ([#11972](https://github.com/EffortlessMetrics/ripr-swarm/issues/11972)),
`break`/`continue` and forced conditions
([#14816](https://github.com/EffortlessMetrics/ripr-swarm/issues/14816)) are also
unknown, as are module constants, a returned `a && b` flipped to `a || b`, and
unary or bitwise edits on a returned name (#14815).

**Python.** A changed module constant reads `no_static_path` even though the
test calls the function that reads it
([#12950](https://github.com/EffortlessMetrics/ripr-swarm/issues/12950)). A
deleted call statement and a body replaced with `None` stay `weakly_exposed`
even under a test that pins them.

**TypeScript.** Module constants read `no_static_path`
([#12950](https://github.com/EffortlessMetrics/ripr-swarm/issues/12950),
[#8568](https://github.com/EffortlessMetrics/ripr-swarm/issues/8568)). Relational
flips, ternaries, loop bounds, `break`/`continue`, `++`/`--`, forced
conditions, arithmetic in a local binding and optional chaining stay
`weakly_exposed` under a pinning test
([#11034](https://github.com/EffortlessMetrics/ripr-swarm/issues/11034),
[#9874](https://github.com/EffortlessMetrics/ripr-swarm/issues/9874)). A
default-parameter change reads `infection_unknown`
([#14817](https://github.com/EffortlessMetrics/ripr-swarm/issues/14817)).

**All three.** Deleting an error guard (`if x < 0 { return Err(..) }`) stays
partial, and so do branch-value changes in ternaries.

## What this does not cover

- One canonical edit per row, in a one-function project. Real code has
  helpers, traits, fixtures and macros; see the
  [verdict corpus](PUBLIC_PROOF.md) for those.
- Whether a mutant is caught at runtime. ripr's verdicts are static. Use
  cargo-mutants, mutmut or Stryker for runtime truth, and
  `cargo xtask mutation-spot-check` for measured agreement.
- Perl. Its probes come from an external emitter, not from ripr's own syntax
  reading.

See also: [Coming from cargo-mutants](FROM_CARGO_MUTANTS.md).
