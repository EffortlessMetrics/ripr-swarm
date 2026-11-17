<!-- section: Fixed -->
- A `return_value` finding on a returned comparison no longer reads `exposed`
  when the only related test input is one ripr cannot see, such as a value
  read from an environment variable or returned by a local helper. For
  `x > 100` changed to `x >= 100` with such a test, the finding is now
  `infection_unknown`, matching the `predicate` finding on the same line. A
  visible literal at the boundary (`thr(100)`) still reads `exposed`
  (#13099).
