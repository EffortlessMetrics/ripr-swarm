# Fixture: typescript_mock_count_arguments

Spec: RIPR-SPEC-0243

Additional contract: RIPR-SPEC-0108. Decision 22; issue #10174.

`notify` rewrites `sender.send(id * 2)` as `sender.send(2 * id)`. The test
asserts `const calls = fn["mock"].calls; assert.deepEqual(calls[0].arguments, [8]);` after passing the node:test mock to the owner.
The changed SideEffect has a removed counterpart. Its reading must be
`exposed`: the argument pin sees 8 versus 5.

Runtime truth was executed with Node v24.19.0, using
`node --experimental-strip-types --test test/notify.test.ts` from `input/`.
Both equivalent spellings pass. Replacing the send argument with `id + 1`
fails the argument assertion, after the module loads and the test runs.
This is an executed control, separate from RIPR static evidence.

Run `cargo xtask fixtures typescript_mock_count_arguments` to exercise the analyzer.
The honesty corpus must reject a contradictory re-bless. No loop-header or
mock-receiver relation claim is made.
