<!-- section: Fixed -->
- A test-local assertion macro whose body calls the owner with its own input
  (`check!(false)` over `assert_eq!(bulk(5), $e)`) is no longer read as the
  call-line assertion, so it can no longer read `exposed` where the written-out
  assertion reads `weakly_exposed` (#10854).
