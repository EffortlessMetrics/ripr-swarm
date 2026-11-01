<!-- section: Fixed -->
- `cargo install ripr` without `--locked` no longer fails to compile `ra-ap-rustc_lexer`. `unicode-properties` 0.1.5 moved to a newer Unicode version than the pinned `unicode-ident` 1.0.24, which trips the lexer's const assert, so the manifest now pins `unicode-properties = "=0.1.4"` beside `unicode-ident` (#11817).
