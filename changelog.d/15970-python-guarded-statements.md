<!-- section: Fixed -->
- Python: a local assignment, a call statement, a `raise` or a continuation
  line of a wrapped call inside an `if` arm that no related test input enters no longer
  reads `exposed`. It now reads `weakly_exposed` and names the guard no input
  satisfies, the same as a `return` in that arm. A test input that enters the
  arm keeps `exposed` (#15970).
