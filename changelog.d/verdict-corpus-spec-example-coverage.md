<!-- section: Added -->
- Verdict corpus: spec-example coverage is now a measured number. A case
  cites the numbered spec acceptance examples it labels in `spec_examples`
  (`RIPR-SPEC-NNNN#K`), and `fixtures/rust-verdict-corpus/spec-coverage.toml`
  scopes all 64 specs with numbered examples (14 in, 50 out), waives 9
  in-scope examples no case can label, and lists 30 prose-example specs as
  unmeasured. The report's new `spec_example_coverage` section reads 71/100
  covered; `cargo xtask verdict-corpus check` fails when covered falls below
  the ledger's floor, and the dx-scoreboard trust board tracks
  `trust.verdict_corpus_spec_example_coverage` against a 100% target
  (#6638).
