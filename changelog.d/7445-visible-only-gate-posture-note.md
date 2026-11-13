<!-- section: Changed -->
- `ripr gate evaluate` in visible-only mode still reads `advisory` when it
  recorded nothing, and its Markdown report now says that the status reflects
  the non-enforcing mode, not a finding, and names the modes that give an
  enforcing `pass` or `blocked`; the gate help and policy docs say the same.
  The JSON report is unchanged (#7445).
