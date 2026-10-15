<!-- section: Fixed -->
- Rust attribute analysis ignores raw-string and comment contents when reading
  FFI attribute paths and `cfg_attr` arguments, and handles wrapper trivia and
  case-sensitive raw identifiers, preserving the appropriate same-language or
  external-oracle repair guidance (#8090).
