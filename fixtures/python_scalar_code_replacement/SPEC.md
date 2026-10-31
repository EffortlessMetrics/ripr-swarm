# Fixture: python_scalar_code_replacement

Spec: RIPR-SPEC-0028

## Given

A scalar module constant read by `price`. The test replaces `price.__code__` with a function that always returns 5 before asserting.

## When

The constant changes from 5 to 6.

## Then

The finding reads `static_unknown`. The replaced code no longer reads the constant, so the assertion holds before and after the edit. A stable name is not a stable implementation: assigning `__code__`, `__defaults__`, `__kwdefaults__` or `__closure__` anywhere in the workspace withholds the reader's credit.

## Must Not

Evaluate the reader's original body when the test can replace its code, or promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native Python runs the unchanged test on the original and edited module: the trap passes both (0 of 1 versions fail), the control fails only after the edit.
