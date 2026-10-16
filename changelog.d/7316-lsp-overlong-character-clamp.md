<!-- section: Fixed -->
- LSP incremental edits clamp a character offset beyond the selected line's length to the line end (before any CR/LF) in UTF-8, UTF-16 and UTF-32, per the LSP Position contract, instead of quarantining the document. Mid-character offsets, absent lines and reversed ranges are still refused (#7316).
