<!-- section: Fixed -->
- `ripr: Show Status` opens the output channel only, without a duplicate
  information toast. The main status-bar item stays hidden when the server is
  unavailable (the missing-server warning already names the recovery) and when
  `ripr.enabled` is false; it still shows `ripr: open workspace` with no folder
  open (#5319).
