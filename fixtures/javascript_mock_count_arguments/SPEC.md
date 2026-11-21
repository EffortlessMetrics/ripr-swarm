# Fixture: javascript_mock_count_arguments

Spec: RIPR-SPEC-0243

Additional contract: RIPR-SPEC-0108. Decision 22; issue #10174 (JavaScript variant).

## Given

`notify` rewrites `sender.send(id * 2)` as `sender.send(2 * id)`. The test
asserts `const calls = fn["mock"].calls; assert.deepEqual(calls[0].arguments, [8]);` after passing the node:test mock to the owner.
The changed SideEffect has a removed counterpart. Its reading must be
`exposed` with `language = "javascript"`: the argument pin sees 8 versus 5.
This is the JavaScript (`.js`) counterpart of `typescript_mock_count_arguments`,
through the TypeScript-family adapter.

## When

Run `cargo xtask fixtures javascript_mock_count_arguments` to exercise the analyzer.
The honesty corpus must reject a contradictory re-bless.

Runtime truth was executed with Node v26.5.0, using
`node --test test/notify.test.js` from `input/`.
Both equivalent spellings pass. Replacing the send argument with `id + 1`
gives 5 for `notify(4)` and fails the argument assertion, after the module
loads and the test runs.
This is an executed wrong-value control, separate from RIPR static evidence.
It is where the 5 in 8 versus 5 comes from: neither committed spelling
(`id * 2` or `2 * id`) sends 5.

## Then

The one anchored SideEffect finding remains `exposed`: the recorded argument
pin distinguishes 8 from 5.

## Must Not

- Treat the recorded argument pin as a count-only observation.
- Relabel JavaScript findings as TypeScript.
- Treat static exposure as runtime mutation evidence or claim complete extraction.
