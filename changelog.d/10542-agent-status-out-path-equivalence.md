<!-- section: Fixed -->
- `ripr agent status --out ./target/ripr/workflow` (and other lexically equivalent spellings of the one accepted directory) no longer fail "not supported": the comparison cleans both sides lexically before comparing, so only genuinely different directories stay refused (#10542).
