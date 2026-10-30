# Fixture: python_call_chain_credit

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0201

## Given

`Ledger.render_ledger` returns `"|" + " " * self._ledger_pad() + "|"`. The only test builds `Ledger()` and asserts `render_ledger() == "|  |"`. No test calls `_ledger_pad`.

## When

`_ledger_pad` changes from `return 2` to `return 3`. Under pytest 9.1.1 the test fails, because the ledger is padded with three spaces.

## Then

One `exposed` finding, related through `module_value_call`. Its evidence names `Ledger._ledger_pad` `2 -> 3` read through `ledger.render_ledger`. The evaluated chain holds with the old body and fails with the new one (#9226).

## Must Not

Report `no_static_path` or the `python_transitive_reach_unresolved` limitation for a chain the evaluator decides.
