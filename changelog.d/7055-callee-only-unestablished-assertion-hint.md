<!-- section: Fixed -->
- Callee-only `reachable_unrevealed` findings whose observe summary is
  the unestablished-assertion marker no longer claim "a related test
  asserts here". The digest hint falls back to "no test is seen calling
  this change", matching the sibling no-assertion arm (#7055).
