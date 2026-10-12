<!-- section: Fixed -->
- Same-test pairing no longer credits a `let`-bound boundary name after the
  test mutates it. `let mut got = gate(10); got = true;
  assert_eq!(got, true)` paired the assertion with a call whose result it
  no longer observed, promoting the predicate toward `exposed`. A post-`let`
  reassignment, compound assignment, or `&mut` borrow now voids the binding
  fail-closed, and the finding reads `weakly_exposed` naming
  `same_test_pairing_missing`. Unmutated `let mut` bindings still pair
  (#7004).
