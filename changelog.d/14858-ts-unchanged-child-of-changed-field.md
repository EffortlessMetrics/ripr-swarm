<!-- section: Fixed -->
- A TypeScript object-literal field change no longer reads `exposed` when the
  test asserts an unchanged child of the changed field: with `retries: 3`
  changed to `retries: { n: 5 }`, `f().retries.other` observes a value the
  change does not alter, so the finding is `weakly_exposed` (#14858). A read of
  a moved child (`f().retries.n`) keeps `exposed`. The alias-local sibling form
  (`const cfg = f(); expect(cfg.verbose)`, #14856) is pinned as
  `weakly_exposed` with a changed-field control.
