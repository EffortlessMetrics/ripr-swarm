<!-- section: Fixed -->
- `ripr check` no longer aborts with a stack overflow on a Rust source whose stray `#"` hides a deep bracket run from the nesting guard; the guard now also reads `#"` the way edition 2024 lexes it (#13494).
