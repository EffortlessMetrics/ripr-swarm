<!-- section: Fixed -->
- MCP tools no longer answer an over-bound success document with a
  text-only envelope that silently drops `structuredContent`. Every tool
  advertises an `outputSchema`, so a document whose complete double-carry
  envelope cannot fit the id-aware response budget now fails closed with
  the typed `result_too_large` refusal carrying that tool's recovery
  route — the same answer the two listing tools already give past their
  paging envelope budget (#7768). The dropped-copy fallback was silently
  reachable on the single-item evidence tools for documents between half
  the response bound and the raw cap, and on `ripr_workspace_status` and
  `ripr_refresh` for any structured-envelope overflow, including ones
  produced by large client-controlled request ids. Successful responses
  are unchanged: they always carry `structuredContent`.
