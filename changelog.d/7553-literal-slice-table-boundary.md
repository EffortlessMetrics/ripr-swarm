<!-- section: Fixed -->
- `ripr check` reads immutable literal slice table cardinalities for direct
  slice-length predicates, so a property table containing only nonempty inputs
  reports its missing empty input. Computed, hidden and skipped inputs retain
  their activation limitation (#7553).
