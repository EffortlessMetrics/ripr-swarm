# RIPR-SPEC-0260: Python guarded-arm activation gate for return-value credit

Status: proposed

Owner: product-analysis

Created: 2026-10-14

Linked issues: [#7392](https://github.com/EffortlessMetrics/ripr-swarm/issues/7392),
[#9002](https://github.com/EffortlessMetrics/ripr-swarm/issues/9002),
[#9184](https://github.com/EffortlessMetrics/ripr-swarm/issues/9184),
[#9429](https://github.com/EffortlessMetrics/ripr-swarm/issues/9429).

Support-tier impact: Python preview adapter output only; no support-tier
change. See: [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md).

Policy impact: no dependency, network, workflow or security-policy changes.

## Problem

A Python return-value change inside a guarding `if` arm executes only when a
call's inputs satisfy the guarding comparisons. Before this gate, a strong
exact-value oracle on inputs that all fail the guard read `exposed`: the
assertion pins the returned value, but the changed line never runs under any
observed input, so the oracle passes either way. Reach plus a strong oracle
is not exposure when the oracle never sees the new line run; the verdict was
a false `exposed` that told the user the change was already pinned.

## Behavior

For a Python `return_value` probe on an added line inside one or more
guarding `if`/`while` arms, the classifier reads the enclosing conditions
from the new-side source and the literal arguments the strong related tests
bind:

- When every strong related call provably fails at least one guarding
  comparison, the changed line never executes under an observed input. The
  finding fails closed to `weakly_exposed`, naming the guarding condition as
  the missing discriminator and surfacing the literal inputs the strong
  calls bind as observed values, the way the predicate boundary rule does.
- A row with an unevaluable guard is never excluded: one unreadable input
  keeps the existing verdict. Shapes that fail open (keep the pre-gate
  verdict) include an `else` that closes a `for`/`while`/`try` rather than an
  `if`/`elif` chain, `elif` arms read by their own condition,
  guards containing `or`/`not` boolean syntax anywhere in the WHOLE
  condition (the check runs before any `and` conjunct is evaluated, so a
  false leading conjunct cannot exclude a row a later `or` lets in,
  #7874), a parameter the owner rebinds before the guard (plain,
  single- or multi-character augmented, annotated, destructuring-target,
  or `for`-target assignment), computed operands, non-literal call rows, a
  strong related call that binds no row (a `*args` unpack or unreadable
  argument list), and entries without source (repo mode, deleted lines,
  the old test harness).
- Fallthrough (#9002): a changed return that follows a simple `if X:` in its
  own block whose body ends in a top-level `return`/`raise` (no `elif`/`else`
  chain) runs only for inputs that fail `X`. When every strong related call
  provably satisfies at least one such preceding early-exit condition, the
  finding fails closed to `weakly_exposed` with missing discriminator
  `not (X)`; the reason says each call satisfies at least one early-exit
  condition, since different calls may take different exits. Lines inside
  triple-quoted strings are not code and are never read as early exits. A
  call that misses every exit, a non-terminating or nested-only `return`
  body, a chained `elif`/`else`, and unreadable guards keep the existing
  verdict.
- Else arm (#9184): a changed line inside the `else:` of an `if`/`elif`
  chain runs only for inputs that fail every chain condition. When every
  strong related call provably satisfies at least one condition of that chain
  (the same readable-comparison evaluation as an early exit), the finding
  fails closed to `weakly_exposed` with missing discriminator
  `not (X)` (one `not (...)` per chain condition, joined by `and`); the reason
  says the line sits in the else arm, which is entered only when none of the
  chain conditions hold, and each strong related test input is kept off it by
  this chain or an enclosing guard. A call that fails every chain condition enters the arm and keeps the
  existing verdict, as do a chain condition containing `or`/`not` or a conditional expression
  (`a if b else c`), an
  unreadable row, and an `else` that closes a `for`/`while`/`try`.
- Equality follows Python semantics for scalar literals: `True == 1` and
  `False == 0` hold, and `2 == True` does not, so a bool call argument is
  not excluded by a numeric guard (or vice versa).
- Ordering comparisons never order across kinds (a string against a number
  is unreadable, which fails open). Integer literals order exactly as
  Python does beyond f64 precision (`9007199254740993 > 9007199254740992`
  holds): both integer-form operands compare as `i128`, refusing on
  overflow, while float literals keep the f64 path because Python rounds
  them at parse time too (#7874).

## Understood guard forms

Beyond `left <op> right` comparisons, `guard_atoms.rs` reads a closed
allow-list of guard forms against the same literal call rows. Each conjunct of
an `and` chain (split outside quotes and brackets) may be:

- `NAME is None` / `NAME is not None`;
- the truthiness of a bare `NAME` (`None`, `False`, `0`, `0.0`, `''` and an
  empty list, tuple, set or dict display are falsy; any other literal and a
  non-empty display are truthy), including a flag that a test omits and so
  takes its literal default;
- `not` over any one of the forms here (`not flag`, `not x is None`);
- `NAME in <literal display>` / `NAME not in <literal display>`, where the
  display is a list, set or multi-element tuple of scalar literals;
- `isinstance(NAME, C)` where `C` is `int`, `float`, `str`, `bool` or a tuple
  of them and none of those builtins is rebound (`isinstance(True, int)`
  holds);
- a `for TARGET in NAME:` header, read as `len(NAME) > 0` over a literal
  display or string, so a loop body never runs for `total([])`.

A row is excluded exactly as for a comparison: the conjunct is provably false.
Everything else keeps the existing verdict, documented as the known limit: a
guard outside this list (`callable(x)`, `x.attr`, a call, a computed operand),
`or`, a name the owner rebinds before the guard, a container the owner may have
changed before the guard (any earlier mention other than a pure condition
test), a non-literal or unbound argument, and an `except` arm. Whether a test
raises is not a literal input, so `except ValueError: return -1` is not
demoted. Plain comparisons and the credit-side reader (#9833) are unchanged.

## Unreached `match` arms and `.get` defaults (#9429)

The same all-rows-excluded rule covers two more branch shapes, in
`arm_activation.rs`:

- A changed line directly in a `case` arm of `match NAME:` where `NAME` is an
  owner parameter the owner does not rebind. A row is excluded when its
  literal binding matches an earlier unguarded arm, or does not match any
  alternative of the changed arm. Patterns read: scalar literals (compared
  with Python `==`, so `True` takes `case 1`), `None`/`True`/`False` by
  identity, builtin `int()`, `float()`, `str()`, `bool()` class patterns,
  `_`, a capture name and `|` alternatives. Any other pattern, or a guarded
  earlier arm, leaves the row not excluded.
- An edit that changes only the literal default of `TABLE.get(key, default)`,
  where `TABLE` is one module-level dict literal of scalar keys that nothing
  in the file but `.get` reads touches and no related test mentions or
  rebinds. A row is excluded when its literal key equals a table key.

Both fail open exactly like the guard rule: an unbound call row, a `*args`
unpack, a non-literal input or any unreadable structure keeps the existing
verdict. A derived observation (`Owner.make().method()` where the edited
constructor argument only reaches the assertion through `method`) is not
covered and still reads `exposed` (#9429, third shape).

## Guarded self-field credit (#9833, #9868)

A separate credit-side reader can admit an ordinary method's guarded
`self.attr` write only when every readable strong call row enters every plain
`if` ancestor. Unreadable rows and other ancestor forms withhold this new
credit. The context-dunder collection shape uses the same entry fact.

This reader uses native Python tokens for known flow escapes and binding
changes. Escaped quotes cannot hide a return, and comments or multiline
literal contents cannot invent an escape. Parenthesized, destructured,
loop-target, declared or named-expression bindings of an input or receiver
withhold credit. Formal parameters of an unused nested definition do not
rebind the containing method's parameter. Non-space code indentation and a
second statement on the changed physical line remain unsupported.

The shared exclusion reader is not a native literal evaluator. Guard credit
therefore withholds floating-point operands and escaped string values,
including resolved call-row and constant operands, rather than assuming raw
literal spelling establishes native equality or mixed numeric ordering.
Unrelated prefix `or`/`not` expressions do not change that decision; these
operators remain unsupported in the actual enclosing guard.

The changed field must also survive explicit owner writes after the changed
line and explicit test writes between the owner call and selected assertion.
Same-named field assignments, augmented assignments, loop/comprehension
targets or deletions, including writes through possible aliases, withhold
credit. Native AST store contexts distinguish these writes from read-only
truthiness conditions, dictionary keys, slices and call arguments. An
annotation without a value does not store to the field. An assignment
containing the owner call stores after that call, including when its target
is spelled first; an earlier semicolon-separated setup remains outside the
observation interval. Different-field writes and read-only uses preserve
credit. A later write on an unresolved receiver can conservatively withhold
credit even when that receiver is a fresh object.
Arbitrary helper effects and general object-flow inference remain outside
this bounded reader. Direct-write retention and exclusion-side decisions are
unchanged.

An observation also follows the completed owner call in source statement order.
Native assertion spans distinguish assertions at different columns on one line;
an earlier semicolon-separated assertion does not observe a later call. The
retention interval belongs to the assertion that earns credit, so an earlier
assertion cannot lend retention to a later masked read. Resolved assertion
helpers keep the exact helper-call span; argument evaluation does not establish
that a field captured before the owner call observes its changed value. This
ordering rule governs the effect-channel credit. The separate owner-result
identity branches (owner name, method on a bound receiver, alias, protocol
entry, module call) do not credit a `self` field or module-object field write when
the owner's text holds no `return` and no `yield` as code, because the call
then hands back only the implicit `None` (or a coroutine or generator) and its
result carries no trace of the write (#10178). Only the effect branch, which
reads the field after the completed call, and the changed-token branch, which
reads the written field itself, may credit such a write. Dunder
owners keep their protocol reading. The fact is textual and fails closed toward
the prior reading: a bare or nested `return` leaves the result branches in
force, and a returned changed field keeps its genuine comparison credit.
The result branches are not a general propagation proof.

## Required Evidence

Unit tests cover the fail-open shapes (`or` and `not` guards whole-condition
and after a false conjunct in both operand orders, a rebound parameter
including multi-character augmented and destructuring-target assignments,
comparison-line negative controls, bool/numeric equality, and exact
large-integer ordering) and the end-to-end classification: an
arm-entering input, an unreadable strong call, a rebinding owner, and each
#7874 counterexample keep `exposed`; only the all-rows-excluded case
downgrades. Fixture goldens record the emitted missing-discriminator text
and observed values.

## Non-Goals

This spec does not change the predicate boundary rule, the boundary rule's
own literal equality (its mixed-kind inequality stays conservative for
credit), or any non-Python adapter. It does not evaluate arbitrary Python
expressions: only simple comparisons over literal bindings are read, and
everything else fails open. It does not cover changes without a readable
guard predicate (a pure-expression change).

## Acceptance Examples

1. `def quote(total): if total > 1000: return round(total * 1.02, 2)` with
   the only strong test asserting `quote(100.0) == 105.0` reads
   `weakly_exposed`, naming `total > 1000` with the observed `total = 100.0`.
2. The same owner with a test asserting `quote(2000.0) == 2040.0` stays
   `exposed`.
3. `if total > 1000 or force:` with a test calling `quote(100.0, True)`
   stays `exposed`: the `or` makes the guard unreadable and the gate fails
   open rather than excluding the row.
4. `def quote(total): total = 2000; if total > 1000: ...` keeps `exposed`
   for `quote(100.0)`: the rebinding means the call binding cannot exclude
   the row.
5. `if flag == 1:` with `quote(True)` stays `exposed` (`True == 1` in
   Python).
6. `def label(x): if x is None: return 'nil'` with the only strong test
   asserting `label(3) == '3'` reads `weakly_exposed`: `x is None` is an
   understood guard form and `3` fails it.
7. The same owner with a test asserting `label(None) == 'nil'` stays
   `exposed`: the guard is provably taken.

## Call-Row Excerpt

The strong call rows the gate reads carry display text (#8777, #8788). The
text is display only and never feeds a decision:

- A physical line of at most 240 bytes is shown whole (trimmed).
- A longer line is cut to a window of at most 240 bytes that starts up to 60
  bytes before the call name, on character boundaries.
- A call that fits the window is kept whole: the lead shrinks by the exact
  call extent (name through the closing parenthesis, whitespace before `(`
  included), and the start rounds toward the call so a multibyte character
  never costs the call its trailing bytes.
- `...` marks a side only where visible text was cut, never for indentation
  or trailing whitespace alone.

## Test Mapping

- `analysis::language::python::guard_activation::tests` covers condition
  reading, the `or`/`not` fail-open, rebound-parameter detection, and
  bool/numeric equality.
- `analysis::language::python::classify::tests::guarded_arm_*` and the
  `or_guard_*`, `unreadable_strong_call_*`, `rebound_parameter_*`, and
  `bool_numeric_guard_*` cases cover the end-to-end verdicts.
- `analysis::language::python::classify::tests::else_arm_*` (including
  `else_text_inside_a_docstring_is_not_an_else_arm` and
  `nested_else_reason_does_not_claim_the_inner_chain_excluded_an_outer_guard_row`)
  cover the else-arm verdicts and reason.
- `analysis::language::python::guard_activation::tests::walk_guards_*` and
  `conditional_expressions_are_not_a_single_comparison` cover the chain walk
  (if/elif reading, no bridging across unrelated headers, loop-else fail-open)
  and the conditional-expression fail-open.

- `analysis::language::python::arm_activation_tests` covers the unreached
  wildcard and literal arms, the controls that reach them, the fail-open
  patterns, and the `.get` default with its table-mutation fail-opens.

- `analysis::language::python::guard_atoms_tests` covers each understood guard
  form, its positive control, and the fail-open shapes.

## Implementation Mapping

- `crates/ripr/src/analysis/language/python/guard_atoms.rs` owns the
  understood non-comparison guard forms the exclusion reader evaluates.

- `crates/ripr/src/analysis/language/python/arm_activation.rs` owns the
  `match` arm and `.get` default exclusion, chained after the guard rule in
  `classify.rs`.
- `crates/ripr/src/analysis/language/python/guard_activation.rs` owns the
  guard reading, row exclusion, and fail-open shapes.
- `crates/ripr/src/analysis/language/python/boundary.rs::strong_owner_call_rows_with_gaps`
  supplies the strong rows and the unbound-call flag the gate refuses on.
- `crates/ripr/src/analysis/language/python/classify.rs` engages the gate
  for `return_value` probes with new-side source.

## Metrics

Report the count of Python return-value findings the gate downgrades per
run; the fixture set carries the expected counts. A false downgrade (a
fixture reading `weakly_exposed` whose observed input enters the arm) is a
release-blocking honesty defect, not a workload measurement.
