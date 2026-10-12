<!-- section: Added -->
- Verdict corpus: `cargo xtask verdict-corpus validate|check|report|bless|split`
  take `--language <language>` and act on
  `fixtures/<language>-verdict-corpus`, so each language's corpus can be
  validated, checked and re-blessed on its own; without it they act on the
  Rust corpus. Only the Rust corpus holds labels to replayable cargo test
  commands and Rust test names (RIPR-SPEC-0245, #6686).
