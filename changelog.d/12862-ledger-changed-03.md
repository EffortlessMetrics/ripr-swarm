<!-- section: Changed -->
- `ripr check` on a diff-scoped format now prints a stderr note when
  unchanged tests are left out of the index (`--no-unchanged-tests` or
  `[analysis] include_unchanged_tests = false`), saying related tests the
  diff does not touch cannot be named and how to restore them. A
  `git_invocation_timeout` error now names `--git-timeout`,
  `RIPR_GIT_TIMEOUT` and the editor's `gitTimeoutMs`, and says `0` disables
  the deadline (#4953, #4946).
- `ripr help --all` now marks the advanced `ripr agent` rows `[advanced]`
  and the `ripr start-here` alias `[compatibility]` (#4962, #4823).
- VS Code: the `ripr.copyTopVerifyCommand` command is now titled "ripr: Copy
  Verify Command (Top Repair Packet)" in the command palette and editor
  context menu, matching the receipt command and saying which packet it
  copies. The command id and keybindings are unchanged (#5235, #5098).
- MCP `ripr_get_receipt_status` reports a finished attempt's terminal
  status with the same receipt reading as `ripr agent status`: a complete
  `changed` receipt leaves the gap open as `limited`, incomplete producer
  completeness reports `limited` even when static movement improved,
  producer-invalid status reports `invalid`, and a receipt's presence alone
  never reports `closed` (#5419, #5199).
- LSP: the default `actionable` profile still leaves out `*_unknown`
  findings but now says so: `ripr/listActionableItems` reports
  `withheld_unknown_count`, the withheld findings and a disclosure sentence,
  `ripr/analysisStatus` reports `withheld_findings` with the
  `diagnostic_profile = full` route, and the refresh log records
  `withheld_unknown=N`. A finding stored with only a line now gets that
  line's full range, so its hover and squiggle appear, and each request the
  server sends to the client is bounded by a 30-second timeout, so a client
  that never answers can no longer leave the server alive but silent
  (#5452, #5276, #5277, #5278).
- Repository seam classification (`ripr pilot` and repo exposure) reads a
  seam as `weakly_gripped` only when its activation is established. A seam
  with weak or missing-discriminator evidence but unknown activation now
  reads `activation_unknown` and keeps its missing-discriminator hint as
  guidance, so pilot no longer ranks it first as a weak-grip gap (#5946).
- `ripr repair` with no eligible seam now prints the next step right after
  its summary sentence and lists the reasons under `Why:`, one per line,
  instead of one long line with the reasons in parentheses and the next
  step last (#11909, #11883).
