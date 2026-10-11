# Fixture: typescript_rejects_reexport_shadow

Spec: RIPR-SPEC-0243

Issue: #7315

## Given

The exported async owner and optional token path are the same as in
`typescript_rejects_literal_equality`. The test re-exports Vitest's `expect`,
which creates no local binding. Its local `expect` helper consumes any rejected
promise and ignores the expected value. The actual test therefore passes with
both `TOKEN_REQUIRED` and the wrong `TOKEN_MISSING` reason.

## When

`ripr check --root input --diff ../diff.patch --format json --mode fast`
exercises the production CLI. The analyzer regression reads this exact test.
For an independent runtime check, copy the positive fixture's input into a
temporary directory, run its pinned `npm ci --ignore-scripts`, replace
`tests/load.test.ts` with this fixture's test, and run `npm test` before and
after replacing the thrown literal. Each run must register one passing test.

## Then

The changed throw remains `weakly_exposed` with an unknown related-test oracle.
A relation-only re-export does not establish an imported runner assertion.

## Must Not

- Grant new exact rejection credit to the local helper.
- Treat a passing wrong-reason runtime control as adequate observation.
- Repair ordinary/global matcher binding behavior, separately owned by #6798.

The inherited return-value classification is preserved here; this control's
honesty row selects only `error_path`, the new credit governed by #7315.
