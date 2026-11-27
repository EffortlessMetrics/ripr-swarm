# Fixture: typescript_vitest_renamed_root_unentered_guard

Spec: RIPR-SPEC-0243

Additional contract: RIPR-SPEC-0108; #16435.

## Given

A Vitest test registers through a renamed root
(`import { test as t } from 'vitest'`). The edit changes the line inside the
`if (coupon === 'HALF')` arm of `price`. The test's only owner call is
`price(2)`, which never enters that arm.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The finding is `reachable_unrevealed`, the same as the test written with
`test(...)`: no related test call enters the changed arm. The finding names
the arm's condition.

The honesty corpus independently rejects promotion to `exposed`, even if
the golden report is regenerated. The adapter tests in
`guard_entry_tests.rs` supply the controls: an entering call, a nested call
through the alias, and a commented owner call keep `exposed`.

## Must Not

- Credit a renamed registration root with entering an arm its calls leave untaken.
- Infer a runtime mutation result from static evidence.
