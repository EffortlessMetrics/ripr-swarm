<!-- section: Internal / CI -->
- Move the existing Kani lane's pure parser and nine parser tests into the shared
  tooling library, preserving lane behavior and giving native observation work
  one interpretation owner to extend (#14472).
- Pin the shared document audit consumer's duplicate-observation refusal even
  when repeating a valid row preserves complete subject-set equality (#14472).
