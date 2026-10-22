<!-- section: Fixed -->
- A predicate finding no longer claims "every related test input short-circuits" when another production function holds the owner without calling it (`let f = owner; f(1, 1)`) or a `macro_rules!` body expands to a call, since either may feed the owner inputs the tests do not show; the reading declines to the ordinary evidence instead (#8462).
