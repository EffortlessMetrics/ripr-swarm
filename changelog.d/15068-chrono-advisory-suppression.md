<!-- section: Security -->
- The dependency advisory check accepts RUSTSEC-2026-0543 (`chrono` is
  unmaintained; no vulnerability, no patched version) until 2026-12-31.
  `chrono` reaches `ripr` only through the MCP SDK `rmcp`, whose published
  releases still require it (#15068).
