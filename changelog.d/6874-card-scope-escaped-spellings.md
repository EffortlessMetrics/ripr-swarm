<!-- section: Fixed -->
- MCP repair-card scope now decodes the stable-path `%` escaping on item
  files before intersecting the seam-inventory corpus, so findings in
  files with a literal `%` (or non-UTF-8 bytes) keep their changed seams
  in card scope instead of dropping out. Wire spellings stay encoded;
  only the encoder's own uppercase `%XX` spellings decode, and distinct
  names never merge (#6874).
