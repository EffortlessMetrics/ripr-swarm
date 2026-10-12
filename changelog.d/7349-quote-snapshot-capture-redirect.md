<!-- section: Fixed -->
- `ripr outcome` and `ripr calibrate cargo-mutants` now quote the capture-command
  redirect path when a missing snapshot path contains spaces, so the printed
  command pastes as one token (#7349). The `read {path} failed: not found.`
  prefix stays unquoted.
