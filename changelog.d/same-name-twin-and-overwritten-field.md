<!-- section: Fixed -->
- A field change no longer reads `exposed` when the only test overwrites
  that field before asserting it (`let q = Quote { total: 99, ..q };`) or
  calls a same-name function it imported from another module
  (`use super::retail::*;`) (RIPR-SPEC-0005).
