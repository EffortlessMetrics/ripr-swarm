<!-- section: Fixed -->
- LSP: one `textDocument/didChange` carrying many tiny ranged edits against a
  large buffer no longer stalls the server. Each ranged position is resolved
  from the start of the retained buffer, so the cost was edits times buffer
  size (in a debug build a 1 MB newline-free buffer with 2000 edits did not answer in 100
  seconds). A change list now has a 256 MiB scan budget; past it the list is
  refused through the existing fail-closed invalid-incremental-change
  quarantine instead of consuming the handler (#11508).
