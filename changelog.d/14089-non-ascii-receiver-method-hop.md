<!-- section: Fixed -->
- `ripr check` no longer loses a file's analysis ("part of the analysis failed
  during language analysis") on source that spells an identifier with
  non-ASCII chars next to a call, path, macro or index, such as
  `р.remise(10)`, `Größe::Bad { .. }`, `prüfe!(..)`, `größe.approx(..)` or
  `日本Result::expect_err(..)`;
  twenty readers in the Rust, Python and TypeScript analysis measured a
  multibyte char as one byte and sliced inside it (#14089).
