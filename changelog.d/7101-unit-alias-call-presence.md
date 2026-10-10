<!-- section: Fixed -->
- Repository inventory keeps a `call_presence` seam for a tail call in a
  function whose return type is a same-module `type X = ();` alias.
  Qualified paths, chained aliases, associated types, same-named type
  parameters, nested functions, `#[cfg]` aliases, `cfg_attr` that
  introduces `cfg`, competing same-name imports, and `const`/`static`
  block shadows stay consumed. A same-named const generic and
  `#[cfg_attr(_, allow(..))]` still count as unit (#7101).
