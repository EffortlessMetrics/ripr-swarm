<!-- section: Internal / CI -->
- `cargo xtask goldens bless` preserves canonical golden bytes and the
  changelog when output matches the gate comparison. Changed Windows output
  is refused before any expected-file or changelog write; apply reviewed
  changes from Linux or WSL instead (#14509, #14510).
