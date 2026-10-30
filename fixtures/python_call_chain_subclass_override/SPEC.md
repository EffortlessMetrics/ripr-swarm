# Fixture: python_call_chain_subclass_override

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

`Wide(Ledger)` overrides `_ledger_pad`. The only test builds `Wide()` and asserts `render_ledger() == "|  |"`.

## When

`Ledger._ledger_pad` changes from `return 2` to `return 3`. The test never runs the changed method, because `Wide` overrides it, so it passes under pytest 9.1.1.

## Then

No `exposed` credit: the call-chain evaluator reads only classes without bases, so `Wide()` decides nothing (#9226).

## Must Not

Credit the edit through the inherited `render_ledger`. The honesty corpus rejects that false promotion independently of the unit tests.
