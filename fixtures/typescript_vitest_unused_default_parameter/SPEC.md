# Fixture: typescript_vitest_unused_default_parameter

Spec: RIPR-SPEC-0243

Additional contract: RIPR-SPEC-0108; #15393.

## Given

A Vitest test imports `pick(name, opts = { level: 'AA' })` and calls it once
with an object literal in the `opts` position, then asserts the exact result.
The edit changes only the default value of `opts` to `{ level: 'AAA' }`. No
other file calls `pick`.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The finding is not `exposed`: every related call passes a value that cannot
be `undefined` at the defaulted position, so the changed default never
applies under the tests and the exact assertion passes on both versions.
The finding names the parameter and the observed call arguments.

The honesty corpus independently rejects promotion to `exposed`, even if
the golden report is regenerated. The adapter tests in
`unused_default_tests.rs` supply the controls: a call that omits the
argument, passes `undefined` or an identifier, or another source caller
keeps the verdict.

## Must Not

- Credit an exact assertion on a call that fills the parameter as observing its default.
- Infer a runtime mutation result from static evidence.
