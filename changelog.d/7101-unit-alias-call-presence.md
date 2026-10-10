<!-- section: Fixed -->
- Repository inventory keeps a `call_presence` seam for a tail call in a
  function whose return type is a same-module `type X = ();` alias.
  Qualified paths, chained aliases, associated types, same-named generic
  parameters, and nested functions stay consumed (#7101).
