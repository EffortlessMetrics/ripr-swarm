# Fixture: typescript_vitest_for_mixed_expect

Spec: RIPR-SPEC-0243

Issue: #10760

## Given

A genuine second-argument `ctx.expect` observes only the literal 1. A separate
bare `expect` is imported from a local fake that never fails. Only the fake
assertion mentions the changed owner and score 50. Lowering the genuine
context call must not grant matcher authority to that unrelated binding.

## When

Run `ripr check --root input --diff ../diff.patch --format json --mode fast`.
Run `npm ci --ignore-scripts` and `npm test` for the native counterexample.

Observed on Linux, Node 24.19.0, Vitest 4.1.11, 2026-11-06: the changed
`score >= 50` owner executes one passing test, zero failures and zero skips.
The genuine assertion constrains an unrelated literal; the fake cannot catch
the boundary fault. The pre-repair candidate reported `exposed` for this input;
its frozen executable SHA-256 is
`ab31202f43749600be87c9feeab2caf99be53f61a33af9f045cec93a0c8947c0`.

## Then

The mixed assertion identities refuse the bounded extraction profile. The
boundary finding remains `no_static_path` with named partial-extraction
disclosure. The independent honesty corpus forbids promotion even if an
incorrect golden is blessed.

## Must Not

- Transfer context authority to a file-level fake `expect` binding.
- Infer that the genuine unrelated assertion observes the owner.
- Count the passing native counterexample as protection for the boundary.
