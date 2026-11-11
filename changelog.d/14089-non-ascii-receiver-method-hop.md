<!-- section: Fixed -->
- `ripr check` no longer loses a Rust file's analysis ("part of the analysis
  failed during language analysis") when a test calls a changed method on a
  receiver spelled with a non-ASCII identifier, such as `р.remise(10)`; the
  method-hop reader measured the char before the receiver as one byte (#14089).
