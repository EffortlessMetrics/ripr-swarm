# Fixture: python_module_string_reader_short_circuit

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A module string constant feeds the single return of `greet`. An unchanged
test contains an exact old-value pin behind a literal false `and` operand:
`assert False and greet('bob') == 'hello, bob'`.

## When

The sole production edit changes `GREETING` from `'hello'` to `'hi'`.

## Then

One `static_unknown` finding retains unknown consumer reach and an unknown
oracle. There is no repair packet or receipt command. Evaluating the test
with either constant raises `AssertionError` without calling the reader;
the exact-looking conjunct does not observe the changed return.

## Must Not

Promote a normalized assertion gate after discarding its false prefix,
report the reader as reached, or remove the finding to claim a clean result.
The honesty corpus independently rejects promotion after a mistaken golden
re-bless. The CPython counterexample is not a pytest-execution or mutation
qualification claim.
