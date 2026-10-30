# False-verdict ledger

Tracking issue: #9430, linked from the 0.11 tracker (#9271).

The ledger is the files, not this page. Every false row has its own file,
`fixtures/<language>-verdict-corpus/known-false/<case>.toml`, with the row's
`case`, the owning `issue` and the `reason` (cause and fix route). Read them
with `ls fixtures/*-verdict-corpus/known-false/`; the per-language count is the
number of files, and `cargo xtask verdict-corpus check-all` prints each
corpus's false rate. This page holds no per-row table or count: both were
shared lines that every fix PR edited, so they conflicted.

`verdict-corpus check` fails on a false row that has no file, and on a file
whose row is no longer false, so a bless cannot absorb a new false verdict and
a fixed row cannot return unnoticed. Fix a row in the case, the spec or the
analyzer. Never exclude or reclassify it. A fix PR deletes its row's file and
touches nothing else in the ledger.

Rows with a long-lived dependency, for orientation:

- `perl-testmore-tier-far-from-boundary` waits on the Perl producer chain
  (#8979, perl-lsp-swarm#17237, then consumer #5562).
- Missed credit (a row that reads `static_unknown` although the tests do
  discriminate) is not a false verdict and is tracked separately (#9619).

## Gate

- `known_false_gate` in `xtask/src/reports/verdict_corpus_known_false.rs`
  runs inside `verdict-corpus check` for a whole-corpus run, which
  `check-all` runs in CI (`rust-gates.yml`, "Verdict corpora"). A case subset
  never trips it.
- Each fix removes its file in the same PR. The removal is the regression
  test: if the row turns false again, `check` reports an unlisted false
  verdict.
