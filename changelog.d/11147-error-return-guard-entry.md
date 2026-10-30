<!-- section: Fixed -->
- A changed `return Err(X)` inside an `if` guard that no related test call
  enters (`if price > 1_000_000` with only smaller prices) no longer reads
  `exposed` because another branch asserts the same variant exactly. It reads
  `weakly_exposed` and names the guard as the missing input (RIPR-SPEC-0263,
  #11147).
