<!-- section: Fixed -->
- `ripr check` no longer reparses a whole function for every changed
  `return` in it when deciding whether a test's expected value can only come
  from that `return`. The function's exits, and whether every `return` leaves
  the function, are now read once per function, so one long function with
  many changed lines runs faster. Findings are unchanged (#11738).
