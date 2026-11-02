<!-- section: Fixed -->
- When a test passes its own empty vector or string as `&mut` to a function
  that pushes into it, and checks only how many items it holds, `ripr check`
  and `ripr repair` no longer say the changed value cannot be followed. Both
  now give the same reason for not starting a repair: the test needs an exact
  check of the full contents it reads back, which ripr cannot write yet
  (#11402).
