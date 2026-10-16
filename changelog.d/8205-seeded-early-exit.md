<!-- section: Fixed -->
- A test that asserts exactly the collection state it seeded before a
  guarded owner call no longer confirms that owner's `push` when the
  owner may exit before pushing; the finding stays `weakly_exposed`,
  while an expectation that includes the pushed element still reads
  `exposed` (#8205).
