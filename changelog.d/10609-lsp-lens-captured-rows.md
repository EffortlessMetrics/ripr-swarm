<!-- section: Fixed -->
- Code lenses, the lens refresh identity and unpublished line hover in Rust files with standalone-CR line endings now use the captured editor row that published diagnostics use, so they no longer sit on an earlier row; LF and CRLF files are unchanged and a later save cannot move a cached lens (#10609).
