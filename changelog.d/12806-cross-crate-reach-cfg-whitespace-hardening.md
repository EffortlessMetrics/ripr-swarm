<!-- section: Fixed -->
- Cross-crate call reach no longer follows a function behind a raw-identifier
  `#[r#cfg(..)]` or a nested `cfg_attr` that adds a `cfg`. The `ripr repair`
  pre-check that decides whether a call may cross a crate also accepts any
  whitespace in `pub fn` declarations (#11401, #12806).
