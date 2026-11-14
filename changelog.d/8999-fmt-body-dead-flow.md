<!-- section: Fixed -->
- Rust test linking: formatting a value no longer credits a changed `Display`/`Debug` `fmt` line, or a helper called only from `fmt`, when that code sits in a literal `if false` block or after a top-level `return` of `fmt` (#8999).
