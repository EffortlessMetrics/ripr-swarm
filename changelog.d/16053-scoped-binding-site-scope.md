<!-- section: Fixed -->
- A macro import or definition inside one function body, such as
  `use MyEnum::*;` in a test, no longer makes every `assert!` and
  `assert_eq!` elsewhere in the file look like a possible macro definition.
  It now affects only calls inside that body, plus calls inside any
  `macro_rules!` definition, since that macro may be invoked there. A
  mention inside an audited `proptest!` or `serde_json::json!` call affects
  no other call. Before, either one could leave every assertion in a large
  test file uncredited (RIPR-SPEC-0197, #16053).
