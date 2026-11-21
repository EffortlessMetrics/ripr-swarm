<!-- section: Fixed -->
- `ripr agent review-summary`, `ripr agent receipt` and `ripr first-action`
  accept the analysis outcome artifact for a workspace whose path contains a
  literal `%`, by decoding the recorded root once with the stable path decoder;
  a workspace whose name only looks encoded still refuses another workspace's
  artifact (#15239).
