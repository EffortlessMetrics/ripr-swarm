<!-- section: Fixed -->
- `ripr reports ts-limitations --check-output` now warns when the supplied
  check output contains findings but none are TypeScript-family findings,
  instead of rendering a silent empty leaderboard that is
  indistinguishable from a genuine zero-limitation result. The warning
  names the skipped finding count and points at the
  `ripr reports gap-ledger --check-output` route, mirroring the sibling
  gap-ledger renderer's skipped-findings warning (#7383).
