<!-- section: Fixed -->
- The gap code action "Write targeted test: open best related test" now
  falls back to the gap diagnostic's anchor line when the ledger repair
  route omits `target_line`, and omits the action when no line is
  available, instead of navigating to line 1 (#7460).
