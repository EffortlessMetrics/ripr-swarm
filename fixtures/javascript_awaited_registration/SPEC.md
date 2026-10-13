# Fixture: javascript_awaited_registration

Spec: RIPR-SPEC-0243

Rule 1: awaited root registration.

## Given

A bounded consumer of Nano ID 6.0.2's real JavaScript module. The published
`non-secure/index.js`, `url-alphabet/index.js`, `package.json`, and MIT license
are retained byte for byte in `upstream/`. [provenance.json](provenance.json)
records the npm archive, its checksum, package commit, and individual file pins.
The input changes only Nano ID's final `return id` to `return id.slice(1)`.

The consumer test is authored here, rather than represented as an upstream
test. It uses the published default 21-character ID contract and Node's real
`await test(...)` ESM registration, with `assert.strictEqual(nanoid().length, 21)`.
No npm dependencies or network access are needed to execute the consumer.

## When

Run `cargo xtask fixtures javascript_awaited_registration`, or:

```bash
ripr check --root fixtures/javascript_awaited_registration/input \
  --diff fixtures/javascript_awaited_registration/diff.patch --json
```

The Rust CLI tests compare the awaited registration to a bare registration,
an awaited suite, a truthiness assertion, skipped and expected-failure roots,
shadowed assertions, and registrations inside initializers and helpers.
A local async `test` helper that discards its callback is a separate adverse
control: its exact assertion never runs and must not supply exposure credit.
A helper named `test` inside a sibling suite cannot hide a genuine top-level
runner registration; the native mutant still fails its exact length assertion.
Forged CommonJS loaders and reassigned runner bindings likewise discard their
callbacks. They remain uncredited while the immutable genuine `createRequire`
consumer retains exact-value credit.
The retained base report is also registered in the shared evidence-promotion
honesty corpus, so re-blessing it as exposed cannot pass that invariant.

For the independent Node contrast, copy `input/` to a scratch directory and run
`node --test test/nanoid.test.js`. Restoring the original upstream module yields
one passing test. The shortened result fails with length 20 versus 21. Replacing
the exact assertion with `assert.ok(nanoid())` passes with the shortened result;
adding `{ skip: true }` produces one skipped test. Labeling used Node v24.19.0.

## Then

The actual CLI finds exactly one current JavaScript return-value probe on line
21. Both bare and awaited exact assertions produce `exposed`, with one related
test and a strong exact-value oracle. The awaited truthiness consumer remains
`weakly_exposed`. Skipped, expected-failure, and shadowed assertions cannot
promote the finding. Initializer and helper registrations remain unsupported.

## Must Not

- Represent the authored consumer as upstream test bytes or a population sample.
- Change or relicense the retained upstream originals.
- Credit a skipped test through a shadowed `undefined` option.
- Claim infection or propagation are established: both remain `unknown`.
- Expand JavaScript beyond the existing preview adapter or emit a repair packet
  without its independently required context.
