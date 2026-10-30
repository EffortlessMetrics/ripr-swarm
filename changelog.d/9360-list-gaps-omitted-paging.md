<!-- section: Changed -->
- `ripr_list_gaps` success documents are now `ripr-mcp-gap-list-v4`. The
  omission disclosure pages on its own axis: `omitted_items` carries at most
  200 identities and 16 KiB per response, the new `omitted_page` discloses the window,
  and `continuation.next_omitted_page` (argument `omitted_offset`) reads the
  next one. A large truncated selection no longer makes the primary discovery
  tool fail with `result_too_large` because of the omission list alone. The
  v3 schema closes its top level, so the added block requires the version
  change, the same precedent as #9604 (#9360).
