<!-- section: Fixed -->
- Rust checks: a dropped or added `&&` / `||` guard operand no longer reads `exposed` from a test whose literal inputs leave that operand at the value that changes nothing (`n != 0 && x` to `x`, pinned only at nonzero `n`) (#8361).
