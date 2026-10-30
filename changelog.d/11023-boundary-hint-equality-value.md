<!-- section: Changed -->
- The boundary assertion placeholder printed by `ripr pilot`, `ripr repair` and the agent packets no longer describes an input that excludes the equality value. For a simple strict comparison it now reads `boundary input where x == 5` instead of `boundary input where x > 5`, matching the `candidate value: 5 (equality boundary)` beside it (#11023).
