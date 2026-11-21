<!-- section: Changed -->
- A repo-wide `ripr check --format repo-exposure-json` cold run, and a cold
  `repo-exposure-summary-json`, classify the compact cache family on every
  core instead of one. On a 4-core host a full rust-lang/cargo scan (28,977
  seams) went from 155 s to 81 s; the documents and cache records are
  unchanged (#15952).
