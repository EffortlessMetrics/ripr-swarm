<!-- section: Fixed -->
- An owner-return pin no longer credits an imported bare call `f(..)` when
  the owner function or an enclosing inline module carries a `cfg`: a
  complementary cfg can compile a same-named `static`, `const` or `use`
  that the call then reaches instead, so the assertion did not prove the
  owner's value (#7082).
