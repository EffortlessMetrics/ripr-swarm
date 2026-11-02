<!-- section: Fixed -->
- Rust item declarations no longer count as function-value references, and a same-name function or import shadows only its own lexical block. Real owner values remain visible beside nested shadows, grouped imports, aliases and declarations on the same line (#10162).
