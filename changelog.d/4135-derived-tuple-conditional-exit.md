<!-- section: Fixed -->
- A derived-local tuple match-arm finding is no longer promoted to `exposed`
  from assertions placed after a conditional or diverging exit in the related
  test (`if ... { return; }`, `?`, `break`, `panic!`/`unreachable!`/`todo!`
  or `process::exit`). The projection oracle now leaves such a test
  unverified, so the arm stays `weakly_exposed`; a complete test with no early
  exit is still promoted (#4135).
