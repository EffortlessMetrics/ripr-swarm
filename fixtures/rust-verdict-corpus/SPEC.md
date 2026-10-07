# Fixture Corpus: rust-verdict-corpus

Spec: RIPR-SPEC-0219

## Given

Pinned excerpts of real Rust crates (serde, regex-syntax, semver, hex,
itoa, bytesize, rusqlite, strsim, atuin; semver and bytesize at two pins) under
`subjects/`, byte-identical to their upstream commits
with license files (Rust sources stored as `.rs.txt`), small authored
crates written to fill cells the real crates leave empty (`authored-pricing`,
`authored-ledger` and `authored-config` for verdict and probe-family cells;
`authored-accounts`, `authored-checkout`, `authored-tokens`, `authored-shop`
and `authored-roles` for test shapes other RIPR specs define, each case naming
its specs in its reasoning; one `authored-specNNNN-<k>` crate per acceptance
example of RIPR-SPEC-0225 to 0228, isolated so no other example's test relates
to its owner; eleven `authored-grid-*` crates crossing cargo-mutants operator
classes with five test styles: exact pin, table-driven loop, property-style
invariant, helper-wrapped assert and no assertion), and one-line edits under
`cases/`. Each case is labeled
with what the crate's own test suite discriminates, established by running
the listed mutants of the edited expression against the full pinned
checkout (for an authored crate, the whole stored crate).

## Layout

`corpus.json` holds only the corpus header. Each subject is
`subjects/<subject_id>.json` beside its retained files, and each case is
`cases/<case_id>.json` beside its `cases/<case_id>.diff`. A file must be
named after the id it holds. The expected state is `expected/summary.json`
(aggregate rates) and one `expected/rows/<case_id>.json` per case. Adding a
case therefore adds files; only the summary's counts are lines another PR
may also change.

## When

`cargo xtask verdict-corpus check` applies each edit to a run-owned copy of
its subject, runs `ripr check --json` on every case in parallel, and
projects the anchored findings to one verdict. `--cases a,b` runs only the
named cases and compares only their rows. The required Rust gate runs
`cargo xtask verdict-corpus check-all`, which checks every
`fixtures/<language>-verdict-corpus`, at Draft -> Ready and on main pushes.

## Then

Each case scores as ideal, abstained, false actionable, false exposed, or
false silent against its label; contradictions inside ripr's own output are
counted; authored rates are reported apart from upstream rates; and every
row must equal its expected row file, the summary must equal
`expected/summary.json`, and `expected/` holds nothing else.

## Must Not

- Run mutation testing, `cargo test`, or network access (`check` and
  `report`; only `relabel` runs test commands, with cargo offline).
- Treat the rates as a population estimate.
- Edit a retained subject file; a changed byte fails its sha256.

## Adding a case

Write `cases/<id>.diff` and `cases/<id>.json`, run
`cargo xtask verdict-corpus check --cases <id>` while iterating, then
`cargo xtask verdict-corpus bless` once to add its row and refresh the
summary. When two case PRs both change `expected/summary.json`, merge main
and run `bless` again; the rows themselves do not conflict.

A branch written against the one-file layout (subjects and cases inside
`corpus.json`) resolves its merge conflict by keeping its own
`corpus.json`, running `cargo xtask verdict-corpus split`, and then `bless`.

## Refreshing

When a ripr change moves a verdict, `check` fails and names each moved
case. Read `target/ripr/reports/verdict-corpus/report.md`. A row
marked `changed_since_labeling` must be re-checked against the full pinned
checkout (for an authored crate, the stored crate itself) before the
expected state is refreshed with `cargo xtask verdict-corpus bless`.

## Re-deriving truth

`cargo xtask verdict-corpus relabel --sample 10` replays ten cases'
mutants against their own test commands and fails on any drift from the
label; `--case <id>` replays one. Each mutant of a behavior-preserving rewrite
must carry `mutated_line`, the trimmed anchor line with the mutant applied,
and `failing_test` must be one exact test name. Upstream cases replay with
`--checkouts <dir>` holding `<dir>/<subject_id>` at the pinned commit, with
its dependencies already fetched, because cargo runs offline. Run it
on every new or relabeled case before opening the PR.
