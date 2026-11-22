<!-- section: Fixed -->
- `RIPR_GENERATED_AT=unix_ms:<millis>` now also pins `written_at` in a
  `ripr receipt write` receipt, a baseline's `created_at`, and
  `generated_at` in the legacy `ripr agent receipt`, `ripr first-action`,
  `ripr assistant-loop`, PR review front panel, comment publish plan, policy
  readiness and report packet documents, so two writes of the same receipt
  or report are byte-identical. A malformed pin refuses the command before
  it writes anything; unset, each still stamps the current time (#14230).
