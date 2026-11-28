# Fixture: typescript_sinon_spy_count

Spec: RIPR-SPEC-0243

Additional contract: RIPR-SPEC-0108. Decision 22; issue #10174.

## Given

`notify` rewrites `sender.send(id * 2)` as `sender.send(2 * id)`. The test
passes a `sinon.spy()` to the owner and asserts `assert.equal(fn.callCount, 1);`.
The changed SideEffect has a removed counterpart.

## When

Run `cargo xtask fixtures typescript_sinon_spy_count` to exercise the analyzer.
The honesty corpus must reject a contradictory re-bless. The fixture imports
`sinon` but does not install it; ripr reads the source statically, and no
runtime execution of this fixture is claimed.

## Then

The one anchored SideEffect finding is `weakly_exposed`.

## Must Not

- Promote a sinon spy count to exact argument credit.
- Change the reading for added calls or other probe families.
- Treat static exposure as runtime mutation evidence or claim complete extraction.
