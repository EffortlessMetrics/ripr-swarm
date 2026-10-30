<!-- section: Changed -->
- `ripr_list_gaps` success documents are now `ripr-mcp-gap-list-v3`. The v2
  success schema closes its top level against unknown properties, so the new
  `outcome` block — the snapshot's typed outcome `kind`, a plain-language
  label, and the outcome limitation kinds — requires this version change, the
  same precedent as the v1 to v2 `filtered`/`filters` addition. A
  `partial_with_limitations` run now discloses its partiality in the list
  itself instead of reading as clean. The advertised `outputSchema`, the
  scope-disclosure schema, docs, and the frozen v1 transport contract are
  updated in step (#9604).
