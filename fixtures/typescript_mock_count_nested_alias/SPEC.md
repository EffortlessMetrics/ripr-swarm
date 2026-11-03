# Fixture: typescript_mock_count_nested_alias

Spec: RIPR-SPEC-0243

Additional contracts: RIPR-SPEC-0246 rule 9 and RIPR-SPEC-0108. Issue #10174.

## Given

`notify` rewrites `sender.send(id * 2)` as `sender.send(2 * id)`. An admitted
`done` callback declares `const calls = fn.mock.calls` and asserts its length.
The alias is unique, straight-line and local to that nested callback.

## When

Run `cargo xtask fixtures typescript_mock_count_nested_alias`.
Runtime truth was executed with Node v24.19.0 using
`node --experimental-strip-types --test bridge.ts` from `input/`.
The retained bridge uses Node built-in test/assert and an explicit minimal
`it/jest.fn/expect.toBe` shim for this numeric count. It is not stock Jest.
Original, equivalent rewrite and wrong `id + 1` arguments all pass after load.

## Then

Exactly one anchored SideEffect finding is `weakly_exposed`: the count stays
one when the recorded argument changes from 8 to 5. Source equality metadata
remains exact/strong; effective observation is weak mock interaction evidence.
The honesty corpus rejects a contradictory exposed re-bless.

## Must Not

- Credit a count as an exact argument pin or admit additional callbacks.
- Change added-call credit, recorded-argument pins or other probe families.
- Let an unused sibling callback's count alias capture an outer argument pin.
- Claim complete Jest compatibility or change loop-header/receiver behavior.
