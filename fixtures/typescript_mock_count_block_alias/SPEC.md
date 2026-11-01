# Fixture: typescript_mock_count_block_alias

Spec: RIPR-SPEC-0243

Additional contract: RIPR-SPEC-0108. Decision 22; issue #10174.

## Given

`notify` rewrites `sender.send(id * 2)` as `sender.send(2 * id)`. Inside an
unconditional lexical block, the Node test declares the unique immutable
`const calls = fn.mock.calls` and asserts its length is one.

## When

Run `cargo xtask fixtures typescript_mock_count_block_alias`.
Runtime truth was executed with Node v24.19.0 using
`node --experimental-strip-types --test test/notify.test.ts` from `input/`.
The original, equivalent rewrite and wrong `id + 1` argument all pass the
count. The same-block `assert.equal(calls[0].arguments[0], 8)` pin passes both equivalent versions
and fails the wrong argument at actual 5 versus expected 8. Both use real
Node built-in test/mock/assert; this is separate from static exposure.

## Then

Exactly one anchored line-2 SideEffect finding is `weakly_exposed`. Source
equality metadata remains exact/strong; effective count evidence is weak.
The honesty corpus rejects an exposed re-bless.

## Must Not

- Credit a unique block-local count alias as an exact argument pin.
- Capture a value read outside the declaring lexical block.
- Weaken same-block argument pins or added-call count credit.
- Change duplicate-name refusal, assertion admission or loop-header handling.
