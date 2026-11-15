<!-- section: Internal / CI -->
- `cargo xtask goldens check-with-fixtures` shares one fixture evaluation between
  fixture and golden reports while retaining golden-only checks; hosted routing
  remains unchanged (#8198).
