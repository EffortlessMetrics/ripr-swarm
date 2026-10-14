<!-- section: Fixed -->
- A cast such as `letters as u32` or a qualified path such as
  `<u32 as Default>` no longer reads as a rename of `u32`. Only `use` and
  `extern crate` items rename now, so a hand-written `Clone` compared with
  its receiver keeps its field pin when some workspace file casts to the
  field's type (#6692).
