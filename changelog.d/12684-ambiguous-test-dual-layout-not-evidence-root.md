<!-- section: Fixed -->
- With `autotests = false`, a name-only `[[test]]` entry that matches both
  `tests/<name>.rs` and `tests/<name>/main.rs` is no longer treated as an
  evidence root, matching Cargo, which drops that target (#6979, #12684).
