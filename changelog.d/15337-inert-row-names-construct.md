<!-- section: Changed -->
- A related test read as `assertion inert` now names the construct that can
  keep its assertion from running, such as "this test's assertion is inside a
  `for` loop, which may run zero times", when all of its refused assertions
  agree on it. Otherwise the row keeps the generic sentence (#15337).
