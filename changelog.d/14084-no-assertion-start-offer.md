<!-- section: Fixed -->
- `ripr check` and bare `ripr repair` no longer offer a repair start for a
  gap the start then refuses because a stage of its static evidence is
  unknown (for example a test that calls the changed function and asserts
  nothing). Bare repair and the named item now give one plain reason, and
  check prints it when its finding is a gap; a finding check already reads as
  unclear keeps its own guidance. The named item refuses as a decision
  (exit 3) instead of "a static limitation with no repairable gap" (exit 2)
  (#14084).
