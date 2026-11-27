# Fixture: typescript_vitest_renamed_import_off_boundary

Spec: RIPR-SPEC-0224

Additional contract: RIPR-SPEC-0027; #16452.

## Given

A Vitest test imports the owner under another name
(`import { discount as applyDiscount } from '../src/price'`). The edit
changes `total > 100` to `total >= 100` in `discount`.
The test asserts `expect(applyDiscount(200)).toBe(180)`, an input on one
side of the boundary only.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The finding stays `weakly_exposed`: the call through the renamed import
reaches the owner, but no input it passes discriminates the changed
comparison.

The honesty corpus independently rejects promotion to `exposed`, even if
the golden report is regenerated. The adapter test
`renamed_owner_import_witnesses_like_the_owner_name` supplies the other
controls: a redeclared alias, an alias on the expected side, and a member
call of another receiver do not witness.

## Must Not

- Credit an off-boundary call through a renamed import with the boundary.
- Infer a runtime mutation result from static evidence.
