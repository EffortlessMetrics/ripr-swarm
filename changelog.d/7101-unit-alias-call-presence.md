<!-- section: Fixed -->
- Repository inventory keeps a `call_presence` seam for a tail call in a
  function whose return type is a same-module `type X = ();` alias.
  Qualified paths, chained aliases, associated types, same-named type
  parameters (`Unit` or `r#Unit`), nested functions, `#[cfg]` aliases
  (including trivia and `r#cfg`), `cfg_attr` that introduces `cfg`,
  competing same-name imports, and `const`/`static` block shadows stay
  consumed. A same-named const generic, `#[cfg_attr(_, allow(..))]`, and
  a one-prefix raw-ident unit alias or return path still count as unit
  (#7101).
