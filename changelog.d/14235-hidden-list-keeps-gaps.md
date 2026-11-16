<!-- section: Fixed -->
- `ripr check` human output: when more than 20 findings are omitted, the
  `Hidden:` list names the highest-priority ones (candidate-current gaps first,
  `exposed` and base-side findings last, the same order the check JSON
  findings budget uses) instead of the first 20 in file order, and the
  remainder line counts what it left out by class (#14233).
