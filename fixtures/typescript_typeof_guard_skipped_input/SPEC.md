# Fixture: typescript_typeof_guard_skipped_input

Spec: RIPR-SPEC-0243

Additional contract: #15261.

## Given

A single-parameter owner returns from inside `if (typeof x === 'object')`. The changed line
is that branch's return (`1` to `99`). The only test calls the owner with
the literal number `5` and pins the fallthrough return `2`.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The literal argument `5` provably fails `typeof x === 'object'`, so the test never enters
the changed branch, and the finding is `reachable_unrevealed` rather than
`exposed`. The exact `toBe(2)` pin on the fallthrough return lends no
credit to the changed line.

The honesty corpus independently rejects promotion to `exposed`, even if
the golden report is regenerated. The production-path test
`array_is_array_and_typeof_guards_withdraw_exposed_for_other_inputs` supplies
the controls: an entering input (`size([1])`) keeps `exposed`, and a
computed input (`size(make())`) keeps the ladder verdict.

## Must Not

- Credit an assertion from a call that cannot enter the changed branch.
- Read a non-literal, defaulted or rebound argument as a known shape.
- Infer a runtime mutation result from static evidence.
