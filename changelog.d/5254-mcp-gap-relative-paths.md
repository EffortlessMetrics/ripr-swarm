<!-- section: Fixed -->
- MCP gap documents (`ripr_list_gaps`, `ripr_get_gap`, gap resources,
  repair fix sites) render file paths relative to the analyzed
  workspace root instead of leaking absolute host paths such as
  `//?/C:/...`. Finding locations use the shared finding-location
  owner (#5996, `./`-prefixed like the check/context/LSP surfaces);
  related-test files and repair fix sites use the
  repository-relative renderer, which tolerates producer spelling
  drift (canonicalized verbatim prefixes, mixed separators, Windows
  case drift); a file outside the root keeps its full stable spelling.
  Root-relative evidence also keeps snapshot identities portable
  across checkouts (#5254).
