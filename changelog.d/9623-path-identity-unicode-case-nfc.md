<!-- section: Fixed -->
- Path identity now folds non-ASCII case on case-insensitive hosts and NFC/NFD spellings on macOS through the shared `path_identity_key` owner, with no new dependency; Linux stays exact and non-UTF-8 paths stay byte-exact (#9623).
- The edit cage's always-case-insensitive tracked-path check now also flags non-ASCII case pairs such as `É.rs`/`é.rs` as ambiguous on Linux (conservative); one-way folds such as the Kelvin sign are never merged with ASCII names (#9623).
