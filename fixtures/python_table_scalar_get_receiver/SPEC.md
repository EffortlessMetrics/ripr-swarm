# Fixture: python_table_scalar_get_receiver

Spec: RIPR-SPEC-0028

## Given

A reader that calls `.get` on its argument, not on the table. The test passes the integer 5.

## When

The silver entry changes from 5 to 6.

## Then

The finding reads `static_unknown`. `5.get(...)` raises `AttributeError` in both the original and the edited code, so the test fails before and after the edit and the default is never read. This is an unsupported-operation case with a failing baseline, not a surviving-mutant result.

## Must Not

Take the `.get` default for a receiver that is not a table, or promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native pytest on the unchanged test raises `AttributeError` on the original and the edited module: an unsupported-operation refusal, not a surviving-mutant claim.
