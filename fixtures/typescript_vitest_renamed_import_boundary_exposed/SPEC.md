# Fixture: typescript_vitest_renamed_import_boundary_exposed

Spec: RIPR-SPEC-0224

Additional contract: RIPR-SPEC-0027; #16452.

## Given

A Vitest test imports the owner under another name
(`import { discount as applyDiscount } from '../src/price'`). The edit
changes `total > 100` to `total >= 100` in `discount`.
The test asserts `expect(applyDiscount(100)).toBe(90)`, the exact changed
boundary input with a live expected value.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The finding is `exposed`, the same as the test written with
`discount(100)`: the call through the renamed import is a call of the owner
and witnesses the changed boundary. No repair asks for the assertion the
test already has.

The honesty corpus holds this as a `must_stay_exposed` control. Its paired
fixture `typescript_vitest_renamed_import_off_boundary` keeps an off-boundary
call through the same alias from promoting.

## Must Not

- Ask for a boundary assertion the test already makes through a renamed import.
- Infer a runtime mutation result from static evidence.
