<!-- section: Fixed -->
- An explicit `return x > 101;` is now read at its boundary like the tail form
  `x > 101`: a Rust `return_value` finding for a changed `return` of one bare
  comparison no longer reads `exposed` when no related test input separates the
  old and new threshold. Comparisons nested in calls, indexes, turbofish or
  compound values keep their reading (#9116).
