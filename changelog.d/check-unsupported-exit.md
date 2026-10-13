<!-- section: Fixed -->
- `ripr check` now exits `2` after emitting an `unsupported_input` report,
  preserving its original findings, counts, input identity and recovery routes.
  This changes the earlier unsupported-input exit-`0` contract; complete,
  empty-scope and `partial_with_limitations` reports remain advisory exit `0`
  (#7440).
