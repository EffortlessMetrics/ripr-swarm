<!-- section: Added -->
- TypeScript related-test rows now carry the `missing_input` miss when the
  row's own closed-body fact says every owner call passes a literal integer
  input off the pinned predicate boundary (#10040, RIPR-SPEC-0224). It
  replaces `observation_unconfirmed` for exactly that state and renders as "no
  input this test passes was seen ...". TypeScript `no_call_path` stays
  withheld: a single file cannot prove absence of a call path.
