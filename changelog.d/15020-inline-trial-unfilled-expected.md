<!-- section: Fixed -->
- A registered `harness = false` inline trial whose expected value is bound
  only to an unfilled placeholder macro (`todo!()`, `unimplemented!()`) no
  longer earns exposure credit through an assertion macro or a terminal
  `return Err(..)` guard, matching ordinary `#[test]` functions (#15020).
