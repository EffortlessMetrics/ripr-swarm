# RIPR-SPEC-0244: Per-language labeled verdict corpora

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

- #6686

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
with the per-record layout, schema (`ripr_verdict_corpus.v1`), label law,
truth table, verdict projection, scoring and per-case expected rows of
RIPR-SPEC-0219. The directory name is the corpus's language; `corpus.json`
carries no language field. `check-all` (RIPR-SPEC-0219) already finds and
gates every such directory, writes each non-Rust report under
`target/ripr/reports/verdict-corpus/<language>/` and runs each language's
cases under `target/ripr/verdict-corpus/<language>/`.

`cargo xtask verdict-corpus validate|check|report|bless|split` take
`--language <language>` and act on `fixtures/<language>-verdict-corpus`;
without it they act on the Rust corpus, as before. A language name is
lowercase ASCII letters, digits, `-` and `_`, not starting with `-`, so it
is a directory component and can be pasted into a command unquoted; any
other name, one whose corpus directory is a symlink, or one that names no
directory holding a `corpus.json`, is refused before anything runs, and `check-all` refuses a corpus directory
whose name gives no such language. `check-all` refuses
`--language`, since it checks every corpus. A drifted `check` names the
`bless --language <language>` command that re-blesses it, and a report's
`report.md` title names its language.

Two RIPR-SPEC-0219 label rules exist so `verdict-corpus relabel` can replay
a Rust case: the test command must be a `cargo` command, and a failing test
must be one Rust test name. `relabel` replays the Rust corpus only, so those
two rules are lifted only for a listed non-Rust language (`typescript`,
`python`, `perl`); every other directory, including `rust-verdict-corpus`, a
misspelled language and a corpus copied elsewhere, keeps the Rust rules. Another
language records the command it ran, and each failing test is one test title:
one non-empty trimmed line, which may hold spaces and commas, as jest and
`node:test` titles do. Every other rule, `mutated_line` included, applies to
every language.

Each language's expected rows are that language's regression gate: any
verdict change fails `check` until the rows are re-blessed with the reason in
the PR. Developer-experience scoreboard metrics derive each language's
false-actionable, false-exposed and false-silent rates from its committed
rows (`verdict-corpus:` sources), so the scoreboard gate compares the
committed rates against its baseline; it does not re-run the corpus.

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
StrykerJS (or, for the two subjects handed over from the Python corpus
thread, `authored-ts-nodetest-pricing` and `authored-ts-vitest-cart`,
mutants applied by hand), recorded per mutant with the failing test, under the
labeling toolchain and test command each case names.

## Required Evidence

- `--language` maps a language to its own corpus directory and refuses a
  name that needs shell quoting, a symlinked corpus directory, or a name
  that names no corpus; it parses
  before or after the subcommand, is refused twice and on `check-all`.
- A report's title and its re-bless hint name the corpus language.
- Only a Rust corpus holds labels to cargo test commands and Rust test
  names; a non-Rust corpus accepts a recorded command and a test title with
  spaces and commas, and still refuses a multi-line, padded or empty title.
- Each committed non-Rust corpus validates, contains both truth directions,
  and its expected rows agree with its labels row by row.

## Non-Goals

- Running mutation testing, a test runner, or network access from the
  harness. Truth is established once, at labeling, and recorded.
- A population estimate. Authored cases are chosen to fill cells.
- Comparing rates across languages; each corpus describes its own cases.

## Acceptance Examples

- `cargo xtask verdict-corpus check` with no `--language` scores
  `fixtures/rust-verdict-corpus` against its unchanged expected rows.
- `cargo xtask verdict-corpus check --language typescript` scores
  `fixtures/typescript-verdict-corpus`, writes
  `target/ripr/reports/verdict-corpus/typescript/report.{json,md}` under a
  `TypeScript verdict corpus report` title, and on drift names
  `cargo xtask verdict-corpus bless --language typescript`.
- `--language cobol` fails before any case runs: `fixtures/cobol-verdict-corpus`
  has no corpus.json.
- `check-all --language typescript` is refused; `check-all` checks every
  corpus.

## Test Mapping

Tests live in `xtask/src/reports/verdict_corpus_tests.rs`:

- `language_names_its_own_corpus_directory_and_nothing_else`
- `language_refuses_a_symlinked_corpus_directory`
- `language_option_parses_in_any_position_and_refuses_misuse`
- `a_report_and_its_rebless_hint_name_the_corpus_language`
- `only_a_rust_corpus_holds_labels_to_cargo_commands_and_rust_test_names`
- `committed_typescript_corpus_is_valid_and_its_rows_agree_with_its_labels`

## Implementation Mapping

- `xtask/src/reports/verdict_corpus.rs` owns the `--language` selection
  and the Rust-only replay label rules; RIPR-SPEC-0219's `check-all` owns
  corpus discovery and per-language run and report paths.
- `fixtures/<language>-verdict-corpus/` holds each language's corpus.

## Metrics

- `verdict_corpus_false_actionable_rate` per language
- `verdict_corpus_false_exposed_rate` per language
- `verdict_corpus_false_silent_rate` per language
