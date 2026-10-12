<!-- section: Fixed -->
- A test that pins a trait default method through a unit struct declared in
  the test's own file, such as `assert_eq!(Unit.advance(), 8)` or
  `let unit = Unit; unit.advance()`, now reads `exposed`. The owner-return
  pin used to type a receiver only from a `let` with a constructor call or a
  struct literal, so the exact assertion read `weakly_exposed`. A bare name
  types the receiver only when nothing else can supply that name: no import,
  rival item, raw identifier or macro input names it (#7083).
- A method named `into_future` no longer pins a trait default through a
  receiver that also implements `Future`: the edition 2024 prelude's
  `IntoFuture::into_future(self)` runs first, so a wrong default passed the
  test while ripr read `exposed` (#7083).
- The same holds for `Iterator`'s by-value comparisons (`eq`, `ne`, `lt`,
  `le`, `gt`, `ge`, `cmp`, `partial_cmp`): a trait default of that name
  reached through an iterator type no longer reads `exposed` (#7083).
  `is_partitioned` is excluded: it is still unstable on the supported
  toolchain, so the name can only spell a custom default and stays credited.
