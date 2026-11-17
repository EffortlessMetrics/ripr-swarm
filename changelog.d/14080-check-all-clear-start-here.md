<!-- section: Changed -->
- `ripr check` says so in plain words when it finds no test gap. On a complete
  run whose every finding reads `exposed`, `Start here` now reads
  `State: no test gap found` and `Safe next action: none needed; tests appear
  to notice every changed behavior ripr checked. This is a static reading of
  the code, not a test run`, instead of `no non-exposed diff finding was
  selected`. When the top finding is `exposed` but others are not, the line
  says none is needed for this change and counts the others. Human text only;
  JSON, the agent format and classification are unchanged (#14080).
