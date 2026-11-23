# Fixture: typescript_vitest_unpinned_literal_arm_return

Spec: RIPR-SPEC-0243

Additional contract: RIPR-SPEC-0108; #15306.

## Given

A Vitest test imports a lookup owner whose every return is a distinct scalar
literal (a `switch` over `1`, `2` and a default). The edit changes the
literal the `case 1` arm returns from `'one'` to `'uno'`. The only exact
assertion calls `dir(2)` and expects `'two'`, another arm's result.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The finding is not `exposed`: no strong assertion expects `'one'`, so each
passes on both versions. The finding names the unpinned arm and the other
arm's expected result.

The honesty corpus independently rejects promotion to `exposed`, even if
the golden report is regenerated. The adapter tests in
`literal_arm_return_tests.rs` supply the controls: an assertion expecting
`'one'` (directly, through a local, or inside an object or array), a
derived observation of the owner's result, an `it.each` table, and a lookup
with a duplicated or computed return keep the verdict.

## Must Not

- Credit an exact assertion on another arm's result as observing the changed arm.
- Infer a runtime mutation result from static evidence.
