# Fixture: typescript_vitest_for_matcher_override

Spec: RIPR-SPEC-0243

Issue: #10760

## Given

A pinned Vitest 4.1.11 test observes `tier(50)` with a literal expected value,
but the imported assertion factory configures `toBe` to always pass. This
replaces even a built-in matcher; the otherwise correlated row supplies no
protection against the changed owner.

## When

Run `ripr check --root input --diff ../diff.patch --format json --mode fast`.
Run `npm ci --ignore-scripts` and `npm test` for the native counterexample.

Observed on Linux, Node 24.19.0, Vitest 4.1.11, 2026-11-06: both the original
`score > 50` and faulty `score >= 50` owner execute one passing test, zero
failures and zero skips. The pre-repair candidate 45b414d reports `exposed`
for this input. Its retained executable SHA-256 is
`4a1deb2ec2c26f6fe2fe388f9542c2144c8ebca52f6bcd56bc0bc3b7c2109930`.
The independent positive fixture retains the real boundary failure under
the unmodified matcher; this counterexample cannot stand in for that proof.

## Then

Visible assertion-factory configuration and value escapes refuse the bounded
profile. The boundary finding remains `no_static_path` with named partial
extraction. The independent honesty corpus forbids promotion even if an
incorrect golden is blessed.

## Must Not

- Infer matcher authority from the built-in method spelling after configuration.
- Restore that authority from a second-argument TestContext sharing matcher state.
- Count a passing test under an always-pass matcher as boundary protection.
- Claim resolution of external runner setup or arbitrary intermodule effects.
