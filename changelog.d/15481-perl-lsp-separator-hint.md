<!-- section: Fixed -->
- `[perl] producer = "perl_lsp"` in `ripr.toml` now ends with ``(did you mean `perl-lsp`?)``. It suggested `perllsp`, because both accepted spellings are one edit away and the first listed won; a value that differs from an accepted one only in its word separator now names that one (#15481).
