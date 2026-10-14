# Fixture Corpus: typescript-verdict-corpus

Spec: RIPR-SPEC-0245 (per-language corpora), with cases drawn from the
RIPR-SPEC-0243 acceptance examples and from test patterns seen in public
TypeScript repositories (re-authored, not copied).

## Given

Thirteen small authored TypeScript packages under `subjects/`, our own code
under this repository's license, grouped by test runner:

- jest (ts-jest): `authored-ts-jest-checkout` (strict and loose matchers,
  boundaries, `toThrow`, mocks, `.not`, `test.each`), `authored-ts-jest-oracles`
  (RIPR-SPEC-0243 oracle examples: snapshots, custom matchers,
  `toMatchObject`, error class and message), `authored-ts-jest-relations`
  (RIPR-SPEC-0243 reach examples: aliased, renamed, default and dynamic
  imports, mocked and unobserved owners), `authored-ts-jest-monorepo` (a
  workspace package), `authored-ts-jest-alias` (a tsconfig `paths` alias) and
  `authored-ts-jest-mined` (default-export objects, test functions passed by
  reference, suite factories, copy-versus-identity equality), plus two
  one-test packages for RIPR-SPEC-0258's committed snapshots:
  `authored-ts-jest-snapshot-unwritten` (no committed `.snap` file) and
  `authored-ts-jest-snapshot-update` (a `jest -u` test script).
- vitest: `authored-ts-vitest-orders` (vitest 5) and `authored-ts-vitest-cart`
  (its older cases labeled under vitest 3.2.7; its `package.json` now pins
  4.1.11).
- mocha with chai: `authored-ts-mocha-ledger` (`expect` and `assert` styles,
  async tests, `.not`, chai `throw` with a message, `deep` equality, and a
  jest-style `toBe` swallowed by `try`/`catch`).
- node:test: `authored-ts-nodetest-billing` and `authored-ts-nodetest-pricing`
  (`node:assert` strict and legacy, `assert.throws` and `assert.rejects`,
  subtests, `t.assert`).

Every RIPR-SPEC-0243 acceptance example has at least one case. Example 33
(two test files, one mocked) is `jest-spec0243-ex33-two-files-one-mocked`;
examples 34 and 35 are `nodetest-spec0243-ex34-throws-anchored-regex` and
`nodetest-spec0243-ex35-rejects-message-changed`, whose edits change the
message the payload pins. Where an example's test pins the post-change value
(examples 30, 34 and 35), the case pins the stored pre-change value instead,
because corpus tests must pass on the stored package; the static oracle grade
the example is about is the same. The older `nodetest-currency-throws-regex` and
`nodetest-charge-rejects-message` keep the message unchanged and cite no
example. Case ids containing `spec0234` were named before the
TypeScript rules were renumbered; they refer to today's RIPR-SPEC-0243.
Where the spec says "(today X)", the case keeps today's verdict in its
labeling observation; where the mutant run contradicts it, the case scores
it as false and its reasoning names the spec example.

Each case that labels a numbered acceptance example cites it in
`spec_examples`, and `spec-coverage.toml` gates those citations on a floor.
In scope are RIPR-SPEC-0243, its amendments RIPR-SPEC-0246 to
RIPR-SPEC-0252, RIPR-SPEC-0254, RIPR-SPEC-0256 and RIPR-SPEC-0258. A case cites an example only when its diff and test have
that example's shape; uncovered examples stay visible in `check`'s coverage
table rather than waived. An example that lists several alternative forms or
outcomes counts as covered when a case labels one of them; the other forms
remain open for later cases without moving the count.

Each subject carries a `ripr.toml` that enables TypeScript beside Rust, the
way a user opts a repository in today.

Each case is one edit under `cases/` (one changed line, except the
`*-throw-second-replaced-line` and `*-throw-uneven-*-block` cases, which
change a `throw` in the same block as a comment line that is reworded,
deleted or joined by a second one), labeled with
what the package's own tests discriminate. A behavior-preserving rewrite
lists the StrykerJS 10 mutants of the edited line (jest, vitest and mocha
runners; node:test through the command runner) and the outcome of each;
a behavior change is its own single mutant. Labels were taken with Node 22, jest 30.5, ts-jest 29.4,
vitest 5.0.3 (3.2.7 for `authored-ts-vitest-cart`, replayed under 5.0.3), mocha 12.0.3, chai 6.3
and tsx 4.23. The two subjects handed over from the Python corpus thread
(`authored-ts-nodetest-pricing`, `authored-ts-vitest-cart`) list hand-applied
mutants instead of Stryker's, except their five boundary rewrites, which list
Stryker's mutant set. All 185 mutant outcomes were then replayed by
applying each mutant by hand and running the case's test command; every one
agreed. The `*-throw-second-replaced-line` and `*-throw-uneven-*-block`
cases (#7524) came later and were labeled by running `npx mocha` on the
stored and the edited package. The twenty spec-coverage cases added after
the ledger (#7650) were labeled the same way: each behavior change was
applied by hand and the case's test command run on the whole stored and
edited package (Node 22.22.0, tsx 4.23.15, jest 30.5.2, ts-jest 29.4.14,
typescript 5.9.3). The four `jest-mined-*-bound-local`, `-error-twice` and
`-name-only` rewrites list, hand-applied, the mutant set StrykerJS listed
for the same line in `jest-mined-tobeinstanceof` and
`jest-mined-tomatchobject-field`. The 37 `*-spec0246-*` to `*-spec0256-*`
cases added after them were labeled the same way, in new source and test
files so no older case's truth could move; the five
`jest-spec0249-ex0[3-8]-*` rewrites list, hand-applied, the same StrykerJS
set for their `if` line (with the block and string mutants a one-line `if`
adds), `jest-spec0248-drop-literal-zero` lists the arithmetic mutant
StrykerJS makes of its `+`, and the `vitest-spec0254-*` cases ran vitest
4.1.11, the version the cart subject's `package.json` pins today; the cart's
older cases keep the 3.2.7 labels recorded above. In `jest-spec0246-ex09-async-foreach` no test reports a failure:
the discarded async callback's failing `expect` is an unhandled rejection
that ends the jest process with exit code 1. The test command fails, which
is what counts as detection, but no test is named as failing, and the case's
`failing_test` says so rather than naming the test whose callback raised it
(#7734).

The five `jest-spec0258-*` cases came last and were labeled the same way.
Their two own subjects exist because each one's whole-package state is the
point: a missing snapshot fails every `jest --ci` run of the unedited
package, so `authored-ts-jest-snapshot-unwritten` runs `jest --ci=false`,
where each run, started from the committed package without a `.snap`
file, writes the entry from what the code returns; and
`authored-ts-jest-snapshot-update` is labeled through `npm test`, which runs
its `jest -u` script and rewrites the committed entry.

## Layout

The per-record layout of RIPR-SPEC-0219: `corpus.json` holds the header,
each subject is `subjects/<subject_id>.json` beside its package, each case
is `cases/<case_id>.json` beside its `cases/<case_id>.diff`, and the
expected state is one `expected/rows/<case_id>.json` per case. Each
behavior-preserving mutant names its `mutated_line`, the anchor line with
the mutant applied, which is the line the replay below wrote. Test commands
are the npm commands each case ran and failing tests are test titles, so
`verdict-corpus relabel`, which replays cargo commands, does not run on this
corpus.

## When

`cargo xtask verdict-corpus check --language typescript` applies each edit
to a run-owned copy, runs `ripr check --json`, and projects the anchored
findings to one verdict. The required gate's `verdict-corpus check-all`
checks this corpus with every other.

## Then

Each case scores as ideal, abstained, false actionable, false exposed, or
false silent, and every row must equal its file under `expected/rows/`.

## Must Not

- Run mutation testing, a JavaScript test runner, `npm install`, or network
  access. The subjects carry no `node_modules`.
- Treat the rates as a population estimate for TypeScript code.

## Known labeling caveat

StrykerJS's vitest runner, with vitest 5, reported mutants that only a test
inside a `describe()` block detects as not detected. `authored-ts-vitest-orders` therefore keeps
its tests at the top level. Run through the command runner on
`authored-ts-vitest-cart`, Stryker reported every mutant of
`ts-vitest-bulk-threshold` as not detected although each fails a plain
`vitest run`; that case records the replayed outcomes. The replay confirmed
every label.

## Refreshing

When a ripr change moves a verdict, `check` fails and names each moved row.
Re-run the case's mutants against the stored subject before re-blessing with
`cargo xtask verdict-corpus bless --language typescript`.

## Relabel history

The awaited-root repair of 2026-10-13 (RIPR-SPEC-0243 rule 1) moves only
`nodetest-spec0234-ex23-await-test` from `limited` / `no_static_path` to
`credited` / `exposed`. The original subject, runtime label and labeling
observation are unchanged. A fresh Node v24.19.0 / tsx 4.23.15 replay passed
all 13 baseline tests and failed only `price` after changing `amount - 10`
to `amount - 20`, with actual 130 versus expected 140. The row records that
its static verdict changed since labeling.

The boundary relabel of 2026-10-04 (#6686, `a985857`) moved twelve
boundary cases: their `behavior_preserving_rewrite` edits became operand
swaps such as `age >= 18` to `18 <= age`, which are equivalent for every
input including fractions and NaN, and their mutants were re-listed for
the new line and replayed. The layout migration that followed
(per-case files, `mutated_line` from the replayed lines) moved no row.
