<!-- section: Fixed -->
- A `ripr explain` / `ripr context --at` miss on a wrong-case path such as
  `SRC/Lib.rs:9` no longer says the file exists with no finding (on macOS
  and Windows) while the next sentence lists that file's findings; only the
  case hint naming the correctly cased file remains (#14676).
