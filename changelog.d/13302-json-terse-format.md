<!-- section: Added -->
- `ripr check --format json-terse` is an opt-in JSON rendering for
  token-priced consumers. Long repeated evidence strings in findings are coded
  as `E0001`-style codes with the full text emitted once in a document-head
  `evidence_legend`, and a finding field that repeats another field's value is
  emitted once with the omitted key listed in `dropped_aliases`. Identity and
  verdict fields are never dropped and the document is stamped
  `"rendering": "terse"`; default `json` and `json-compact` output is
  unchanged. On the 11-finding `returned_closure_arithmetic_masking` fixture
  the output is 91,971 B against 114,611 B for `json-compact` and 157,293 B for
  `json`. The published check schema gains the three optional head properties
  (#13302).
