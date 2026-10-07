<!-- section: Fixed -->
- An owner-return pin no longer credits an imported bare call `f(..)` when
  the owner function, an enclosing module (inline, or an out-of-line `mod`
  declaration that compiles the owner's file) or the file itself carries a
  `cfg`: a complementary cfg can compile a same-named `static`, `const`,
  `use` or module that the call then reaches instead, so the assertion did
  not prove the owner's value (#7082).
