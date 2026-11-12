<!-- section: Fixed -->
- A change inside one impl of a generic trait no longer reads `exposed` from a
  test that only calls a sibling impl on the same type. With
  `impl From<u32> for Cents` beside `impl From<&str> for Cents`, a test whose
  every call passes a value written as the other impl's argument type
  (`Cents::from(2u32)`, `Cents::from("abc")`, a typed `let`, a struct literal)
  is a name-only relation for the impl it cannot run. A call whose argument
  type is not written in the test keeps its relation (#14074).
