<!-- section: Fixed -->
- `ripr check` no longer re-reads a whole function for every changed
  `return` in it when deciding whether a test's expected value can only come
  from that `return`. The function's exits, and whether every `return` leaves
  the function, are now read once per function, so one long function with
  many changed lines no longer costs time that grows with the square of its
  length there. Findings are unchanged (#11738).
