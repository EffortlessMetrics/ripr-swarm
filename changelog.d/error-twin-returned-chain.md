<!-- section: Fixed -->
- Wrapped error constructors now get one `error_variant` seam, on the
  constructor itself, instead of a second seam on the wrapper:
  - a `return` around an `Err(..)` with only annotating methods chained on
    it (`context`, `with_context`, `wrap_err`, `inspect_err`, `into`, ...)
    (`return Err(X).context(..)`, #6935);
  - a call that takes the constructor as a top-level argument, including
    nested wrappers (`Poll::Ready(Err(X))`, `Ok(Poll::Ready(Err(X)))`,
    `pick(s, Err(A), Err(B))`, `wrap(Error::X(1))`, #6938).

  Wrappers that add error behavior of their own keep their seams: a chain on
  a non-`Err` shape (`return load(Error::A).map_err(Error::Io)`), a function
  on a type (`io::Error::new(kind, Error::W(1))`), a capitalised constructor
  around an `Err(..)` unless it is a plain wrapper such as `Ok`, `Some`,
  `Poll::Ready` or `Box::new` (`Error::Outer(Err(X))`, `Self::Outer(..)`), a
  returned chain with any other method, which may replace the error
  (`return Err(A).map_err(|_| Error::B)`, `.or_else::<E, _>(..)`,
  `.recover()`), a longer chain on the
  call (`wrap(Err(X)).map_err(..)`), and a call around such a chain
  (`Ok(wrap(Err(X)).map_err(..))`). A `return x.map_err(..)` with no
  constructor inside keeps its seam too. Classified seam caches move to
  1.44 / 0.50, so warm entries rebuild (#6935, #6938).
