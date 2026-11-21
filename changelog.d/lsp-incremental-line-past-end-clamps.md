<!-- section: Fixed -->
- `ripr lsp` treats an incremental `didChange` position whose line lies past
  the last line of the document as the end of the document, as it already did
  for a character past the line end, instead of quarantining the document and
  withdrawing its diagnostics (#15430).
