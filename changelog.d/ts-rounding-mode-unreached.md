<!-- section: Fixed -->
- TypeScript: swapping one `Math.round` / `Math.floor` / `Math.ceil` /
  `Math.trunc` for another no longer reads `exposed` when every related
  test's literal input folds the pre-rounding value to a whole number (or one
  both functions round alike). The finding reads `weakly_exposed` and asks
  for a case whose pre-rounding value is fractional; any unreadable call
  keeps the previous verdict (RIPR-SPEC-0248 rule 2, #9437).
