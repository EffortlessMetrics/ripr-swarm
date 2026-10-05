# Fixture Corpus: typescript-verdict-corpus

Spec: RIPR-SPEC-0238 (per-language corpora), with cases drawn from the
RIPR-SPEC-0234 acceptance examples and from test patterns seen in public
TypeScript repositories (re-authored, not copied).

## Given

Eleven small authored TypeScript packages under `subjects/`, our own code
under this repository's license, grouped by test runner:

- jest (ts-jest): `authored-ts-jest-checkout` (strict and loose matchers,
  boundaries, `toThrow`, mocks, `.not`, `test.each`), `authored-ts-jest-oracles`
  (RIPR-SPEC-0234 oracle examples: snapshots, custom matchers,
  `toMatchObject`, error class and message), `authored-ts-jest-relations`
  (RIPR-SPEC-0234 reach examples: aliased, renamed, default and dynamic
  imports, mocked and unobserved owners), `authored-ts-jest-monorepo` (a
  workspace package), `authored-ts-jest-alias` (a tsconfig `paths` alias) and
  `authored-ts-jest-mined` (default-export objects, test functions passed by
  reference, suite factories, copy-versus-identity equality).
- vitest: `authored-ts-vitest-orders` (vitest 5) and `authored-ts-vitest-cart`
  (vitest 3.2).
- mocha with chai: `authored-ts-mocha-ledger` (`expect` and `assert` styles,
  async tests, `.not`, chai `throw` with a message, `deep` equality, and a
  jest-style `toBe` swallowed by `try`/`catch`).
- node:test: `authored-ts-nodetest-billing` and `authored-ts-nodetest-pricing`
  (`node:assert` strict and legacy, `assert.throws` and `assert.rejects`,
  subtests, `t.assert`).

Every RIPR-SPEC-0234 acceptance example (1 to 32) has at least one case.
Where the spec says "(today X)", the case keeps today's verdict in its
labeling observation; where the mutant run contradicts it, the case scores
it as false and its reasoning names the spec example.

Each subject carries a `ripr.toml` that enables TypeScript beside Rust, the
way a user opts a repository in today.

Each case is a one-line edit under `cases/`, labeled with what the package's
own tests discriminate. A behavior-preserving rewrite lists the StrykerJS 10
mutants of the edited line (jest, vitest and mocha runners; node:test through
the command runner) and the outcome of each; a behavior change is its own
single mutant. Labels were taken with Node 22, jest 30.5, ts-jest 29.4,
vitest 5.0.3 (3.2.7 for `authored-ts-vitest-cart`), mocha 12.0.3, chai 6.3
and tsx 4.23. All 115 single-line Stryker outcomes outside the two handed-over
subjects were replayed by applying the mutant by hand and running the test
command; every one agreed.

## When

`cargo xtask verdict-corpus check --language typescript` applies each edit
to a run-owned copy, runs `ripr check --json`, and projects the anchored
findings to one verdict.

## Then

Each case scores as ideal, abstained, false actionable, false exposed, or
false silent, and the report must equal `expected/report.json` and
`expected/report.md`.

## Must Not

- Run mutation testing, a JavaScript test runner, `npm install`, or network
  access. The subjects carry no `node_modules`.
- Treat the rates as a population estimate for TypeScript code.

## Known labeling caveat

StrykerJS's vitest runner, with vitest 5, reported mutants killed only inside
a `describe()` block as survived. `authored-ts-vitest-orders` therefore keeps
its tests at the top level; the replay confirmed every label.

## Refreshing

When a ripr change moves a verdict, `check` fails and names the first
differing line. Re-run the case's mutants against the stored subject before
re-blessing with
`cargo xtask verdict-corpus report --language typescript --out fixtures/typescript-verdict-corpus/expected`.
