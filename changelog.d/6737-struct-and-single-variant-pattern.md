<!-- section: Fixed -->
- `assert!(matches!(cfg(), Cfg { .. }))` and `assert!(matches!(make(),
  Only::Value))` read `relational_check` / weak when the test file uniquely
  declares `struct Cfg` or `enum Only { Value }` (RIPR-SPEC-0231 rule 3,
  examples 24 and 25; #6737). An unresolved path still keeps today's
  `exact_value` / strong reading.
