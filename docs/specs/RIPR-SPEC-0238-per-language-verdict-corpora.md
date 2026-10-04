# RIPR-SPEC-0238: Per-language labeled verdict corpora

Status: proposed

Owner: product-eval

Created: 2026-10-04

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #5506 (TypeScript custom matchers)

Linked PRs:

- None yet

Support-tier impact:

- None. A corpus measures verdict accuracy on named cases. It does not
  promote a language, editor surface, gate, or public support claim.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- Add `fixtures/typescript-verdict-corpus`, `fixtures/python-verdict-corpus`
  and `fixtures/perl-verdict-corpus` to the manifest-only fixture set.
- Authored TypeScript, Python and Perl sources are fixture data under
  `fixtures/**`, which the non-Rust allowlist already covers. No new crates,
  binaries, dependencies, network allowlist rows, or process spawn sites.

## Problem

[RIPR-SPEC-0219](RIPR-SPEC-0219-labeled-rust-verdict-corpus.md) measures how
often ripr's Rust verdict is wrong against runtime mutant truth. ripr also
analyzes TypeScript, Python and Perl, and none of those verdicts has a
labeled corpus, so a change to a TypeScript oracle or reach rule cannot show
that it did not make another TypeScript verdict wrong. Folding other
languages into the Rust corpus would make every label change in one language
re-bless the expected report of all of them, and would mix rates that
describe different analyzers.

## Behavior

Each language has its own corpus directory, `fixtures/<language>-verdict-corpus`,
with the same layout, schema (`ripr_verdict_corpus.v1`), label law, truth
table, verdict projection and scoring as RIPR-SPEC-0219.

`corpus.json` carries a `language`: one of `rust`, `typescript`, `python`,
`perl`. An absent `language` reads as `rust`, so the Rust corpus is
unchanged. The validator refuses any other value.

`cargo xtask verdict-corpus validate|check|report` take
`--language <language>` (default `rust`) and read
`fixtures/<language>-verdict-corpus`. A directory whose `corpus.json` declares
a different language is refused, so one language's corpus can never be
scored under another's name. Rust keeps its run path
(`target/ripr/verdict-corpus/`) and report path
(`target/ripr/reports/verdict-corpus/`); every other language nests under
`<language>/` in both, so two corpora never share a run copy or a report.
The re-bless hint a drifted check prints names the `--language` it needs.

The report for a non-Rust corpus names its `language` in `report.json` and in
the `report.md` title. The Rust report omits the field and keeps its title,
so its expected report keeps its bytes.

Each language's expected report is that language's regression gate: any
verdict change fails `check` until the expected report is re-blessed with the
reason in the PR. Developer-experience scoreboard metrics read each
language's false-actionable, false-exposed and false-silent rates from its
expected report.

### TypeScript corpus

Subjects are authored packages (`authored-ts-<library>-<name>`), our own code
under this repository's license, each a minimal package with a
`package.json` naming its test library, a `ripr.toml` enabling the
TypeScript adapter, production sources and tests. Cases cover the test
libraries in usage order (jest, vitest, mocha with chai, `node:test`) and
the oracle shapes ripr's TypeScript adapter distinguishes: exact-value and
structural matchers, truthiness and broad matchers, thrown and rejected
errors, async tests, snapshot tests, custom matchers (#5506), mocks, and
boundary inputs. Truth is a real mutation run of the anchored line with
StrykerJS (or, for a test library Stryker cannot drive, the same mutants
applied by hand), recorded per mutant with the failing test, under the
labeling toolchain and test command each case names.

### Python corpus

Subjects are authored projects (`authored-py-<library>-<name>`) with a
`pyproject.toml` or `setup.cfg` marker and no `ripr.toml`, so Python is
enabled the way a new user's repository enables it. Cases cover pytest
(asserts, `parametrize`, `raises`, fixtures, `capsys`, `tmp_path`,
`monkeypatch`), unittest (assert methods, mocks, mixins) and Hypothesis
(`@given`, `@example`, `assume`), plus one case per RIPR-SPEC-0233
acceptance example that can run. Truth is the same hand-applied mutant run
the Rust corpus uses, under the test command each case records.

## Required Evidence

- A corpus without `language` reads as Rust and its rendered report has no
  `language` field and the Rust title.
- A non-Rust corpus names its language in both report files.
- The validator refuses an undeclared language.
- `--language` maps each language to its own directory and run paths and
  refuses anything else; a directory declaring another language is refused.
- Each committed non-Rust corpus validates, contains both truth directions,
  and its expected report agrees with its labels row by row.

## Non-Goals

- Running mutation testing, a test runner, or network access from the
  harness. Truth is established once, at labeling, and recorded.
- A population estimate. Authored cases are chosen to fill cells.
- Comparing rates across languages; each corpus describes its own cases.

## Acceptance Examples

- `cargo xtask verdict-corpus check` with no `--language` scores
  `fixtures/rust-verdict-corpus` and compares against its unchanged expected
  report.
- `cargo xtask verdict-corpus check --language typescript` scores
  `fixtures/typescript-verdict-corpus`, writes
  `target/ripr/reports/verdict-corpus/typescript/report.{json,md}`, and on
  drift names `report --language typescript --out
  fixtures/typescript-verdict-corpus/expected` as the re-bless command.
- `--language typescript` pointed at a directory whose `corpus.json` says
  `rust` fails before any case runs.

## Test Mapping

Tests live in `xtask/src/reports/verdict_corpus_tests.rs`:

- `a_corpus_without_a_language_is_rust_and_its_report_keeps_its_bytes`
- `a_non_rust_corpus_names_its_language_in_both_reports`
- `validator_rejects_an_undeclared_language`
- `each_language_owns_its_corpus_directory_and_run_paths`
- `a_language_directory_must_declare_that_language`

## Implementation Mapping

- `xtask/src/reports/verdict_corpus.rs` owns the language field, the
  `--language` selection, and per-language paths.
- `fixtures/<language>-verdict-corpus/` holds each language's corpus.

## Metrics

- `verdict_corpus_false_actionable_rate` per language
- `verdict_corpus_false_exposed_rate` per language
- `verdict_corpus_false_silent_rate` per language
