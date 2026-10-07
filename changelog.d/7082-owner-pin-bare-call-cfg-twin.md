<!-- section: Fixed -->
- An owner-return pin no longer credits an imported bare call `f(..)` when
  a `cfg` can drop the owner: on the function, an enclosing inline module,
  the owner's file or a file above it, or the `mod` declaration (including
  a `cfg_attr` `#[path]`) that compiles it. A complementary cfg can compile
  a same-named `static`, `const`, `use` or module that the call then
  reaches instead, so the assertion did not prove the owner's value. A
  `cfg_attr` that only toggles lints or docs keeps the pin (#7082).
