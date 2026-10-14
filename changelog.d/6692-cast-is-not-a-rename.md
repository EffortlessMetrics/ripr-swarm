<!-- section: Fixed -->
- A cast such as `letters as u32` no longer reads as a rename of `u32`, and
  a qualified path such as `<Window as Clone>` no longer reads as a rename
  of `Clone`. Only `use` and `extern crate` items, macro arguments and
  attributes can rename now, so a hand-written `Clone` compared with its
  receiver keeps its field pin when some workspace file casts to the
  field's type (#6692).
