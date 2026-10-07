<!-- section: Fixed -->
- `ripr mcp` no longer fails a budget-approved `ripr_list_gaps` listing with
  `result_too_large` on the wire: tool responses carry the document once
  (compact `content[0].text`, with `structuredContent` kept whenever the
  complete envelope measures under the 128-KiB bound), and `ripr_list_gaps`
  accepts `offset`/`limit` paging that byte-fits each page and discloses the
  window through `page`, so a hundreds-of-gaps workspace is enumerable
  (#6021).
