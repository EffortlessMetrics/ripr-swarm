<!-- section: Fixed -->
- An audited verification-prelude glob (`use vstd::prelude::*;` and the other
  RIPR-SPEC-0369 globs) no longer credits assertions in a file where a plain
  import rebinds the crate name (`use other::vstd;`). The glob then reads the
  imported module, which may export its own `assert_eq!`, so it is treated as
  a foreign glob, as the standard-root globs already were. An absolute
  `::vstd::prelude::*` path is unaffected (#14116).
