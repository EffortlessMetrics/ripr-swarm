<!-- section: Fixed -->
- `ripr agent start --seam-id <unknown>` now exits `3` as a typed refusal,
  like `agent card`, `agent stub` and `agent packet` for the same cause, instead
  of exit `2`; other `agent start` failures still exit `2` (#10537).
