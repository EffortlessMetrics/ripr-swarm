<!-- section: Fixed -->
- `ripr check` no longer contradicts itself when a related test has an exact
  assertion ripr cannot confirm observes the changed expression: the
  missing-check line now names the unconfirmed observation link instead of
  claiming no strong assertion was detected, the Next step stops prescribing
  the exact assertion the test already has, and `ripr explain` replaces the
  unactionable "assert on the changed value itself, by name" recipe for
  returned values; verdicts are unchanged (#13037).
