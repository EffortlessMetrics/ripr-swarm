# Fixture: python_table_code_replacement

Spec: RIPR-SPEC-0028

## Given

A table read by `price`. The test replaces `price.__code__` with a function that always returns 5 before asserting.

## When

The silver entry changes from 5 to 6.

## Then

The finding reads `static_unknown`. The replaced code no longer reads the table, so the assertion holds before and after the edit. A stable name is not a stable implementation: `__code__`, `__defaults__`, `__kwdefaults__` and `__closure__` withhold the table credit.

## Must Not

Evaluate the reader's original body when the test can replace its code, or promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures; the verdict corpus (`authored-py-table-traps`) carries the runtime truth. RIPR-SPEC-0108 pins the finding independently of golden blessing.
