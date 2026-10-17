<!-- section: Fixed -->
- `ripr pr-ledger record` help, `ripr help --all`, and the `help --json`
  example now disclose that at least one of `--gate`, `--baseline-delta`,
  `--zero-status`, `--pr-guidance`, or `--gap-ledger` is required, and the
  shipped example includes one, so the documented invocation no longer exits 2
  when typed verbatim (#7746).
