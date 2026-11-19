<!-- section: Added -->
- `ripr -v check` prints how long each phase took and one `ripr: verdict`
  line per reported finding (at most 50) naming its class, the five stage
  states and the first stage that is not `yes` with the reason ripr recorded.
  `RIPR_VERBOSE=1` turns on the same stderr diagnostics for editors, MCP
  hosts, hooks and CI without changing stdout, and the troubleshooting guide
  explains how to attach the log to a bug report (#14831, #14832).
