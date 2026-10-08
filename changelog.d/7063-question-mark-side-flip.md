<!-- section: Fixed -->
- A `?` that is its function's only way to return `Err` is now credited
  `exposed` when a test asserts the call fails (`is_err()`, `!is_ok()`,
  `matches!(.., Err(_))`), because swallowing the `?` would make that
  call return `Ok` (RIPR-SPEC-0227 rule 3b). A `?` beside another error
  source, one that maps the error, or one whose call the diff changed
  keeps its `weakly_exposed` gap (#7063).
