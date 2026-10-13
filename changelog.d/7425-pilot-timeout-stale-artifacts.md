<!-- section: Fixed -->
- `ripr pilot` timeout packets no longer leave `repo-exposure.{json,md}` or
  `agent-seam-packets.json` from an earlier complete run in the same directory.
  The timeout path removes those unwritten siblings before writing the partial
  summaries so they cannot be read as this run's outputs (#7425).
