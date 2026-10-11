<!-- section: Changed -->
- Verdict corpus: `verdict-corpus relabel` receipts (`relabel.json`) now
  record `git_head` and a `sha256` digest of `cases/` and `subjects/`, so a
  replay names the corpus state it ran. Schema version is
  `ripr_verdict_corpus_relabel.v2`. Specs 0225–0227 now point at
  `fixtures/rust-verdict-corpus/cases/` (#6945).
