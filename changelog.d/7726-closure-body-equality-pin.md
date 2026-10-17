<!-- section: Fixed -->
- An iterator `.all` / `.any` closure whose body pins the bound element with
  `==` beside a `matches!` (`xs.iter().all(|r| matches!(r, Ok(_)) && *r == Ok(3))`)
  now reads as an exact oracle. Bare `.all(|x| x == 1)`, unknown callees,
  `.position`, self-comparisons, disjuncts, method calls, and foreign names
  stay weak (#7726).
