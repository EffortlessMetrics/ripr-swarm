<!-- section: Added -->
- Verdict corpus: `cargo xtask verdict-corpus validate|check|report` take
  `--language <rust|typescript|python|perl>` and read
  `fixtures/<language>-verdict-corpus`, so each language keeps its own
  runtime-labeled cases, expected report and regression gate. A corpus
  without `language` is Rust, and the Rust report keeps its bytes
  (RIPR-SPEC-0233).
