<!-- section: Fixed -->
- `ripr check` and bare `ripr repair` no longer offer a repair start for a
  gap the start then refuses because a stage of its static evidence is
  unknown (for example a test that calls the changed function and asserts
  nothing). All three now give the same plain reason, and the named item
  refuses as a decision (exit 3) instead of "a static limitation with no
  repairable gap" (exit 2) (#14084).
