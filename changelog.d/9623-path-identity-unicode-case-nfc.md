<!-- section: Fixed -->
- Path identity now folds non-ASCII case on case-insensitive hosts and NFC/NFD spellings on macOS through the shared `path_identity_key` owner, with no new dependency; Linux stays exact and non-UTF-8 paths stay byte-exact (#9623).
