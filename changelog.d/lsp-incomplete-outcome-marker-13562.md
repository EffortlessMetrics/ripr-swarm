<!-- section: Fixed -->
- LSP: an incomplete analysis outcome that publishes no row no longer leaves the
  editor on an empty diagnostic list that reads as an all-clear. When the
  snapshot's outcome is `partial_with_limitations`, `unsupported_input` or
  `analysis_failed` and it holds no finding, gap row or stored diagnostic (for
  example a `diff_scope_oversized` or `malformed_diff` limitation), each document
  the client asks about carries one passive Information marker under the
  existing `ripr-diagnostic-language-not-analyzed` code (`data.kind`
  `incomplete_analysis`) that says the analysis is incomplete, names up to three
  limitations with the same plain labels and recovery text `ripr check` prints,
  and is a static advisory, not runtime evidence. It reuses the empty-scope
  marker lifecycle (late open, close, root change, shutdown, result ids), carries
  no finding identity, repair data or actionability, and never stacks on a row,
  the empty-scope marker or a per-file marker (#13562).
