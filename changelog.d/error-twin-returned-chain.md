<!-- section: Fixed -->
- Wrapped error constructors now get one `error_variant` seam, on the
  constructor itself, instead of a second seam on the wrapper:
  - a `return` around a method chain on the constructor
    (`return Err(X).context(..)`, #6935);
  - a call that takes the constructor as a top-level argument
    (`Poll::Ready(Err(X))`, `Ok(Err(X))`, `pick(s, Err(A), Err(B))`,
    `wrap(Error::X(1))`, #6938).

  A `return x.map_err(..)` with no constructor inside keeps its own seam. A
  capitalised callee that builds its own error around another one
  (`Error::Outer(Error::Inner(1))`) keeps both. Classified seam caches move
  to 1.44 / 0.50, so warm entries rebuild.
