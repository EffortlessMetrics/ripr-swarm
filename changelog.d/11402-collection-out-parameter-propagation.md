<!-- section: Fixed -->
- When a test passes its own empty vector or string as `&mut` to a function
  that pushes into it, and checks only how many items it holds, `ripr check`
  and `ripr repair` no longer say the changed value cannot be followed. Both
  give the same reason when they do not start a repair: the test needs an
  exact count that would fail if nothing were pushed, and until then the full
  contents can be checked by hand (#11402).
