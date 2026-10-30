<!-- section: Fixed -->
- MCP scope disclosure (the `scope` block of `ripr_refresh` results and
  `ripr_workspace_status` session facts) no longer hardcodes "Rust" when
  naming gitignored excluded files. The wording now derives the analyzed
  language set from the committed snapshot's findings — "Rust", "Python",
  "TypeScript", "mixed-language", or "source" when zero findings carry no
  language evidence — and the `ignored_edits` block carries a
  `languages_analyzed` field so clients can render it without parsing prose
  (#9604).
