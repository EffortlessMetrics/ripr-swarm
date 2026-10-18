<!-- section: Fixed -->
- Serialized four LSP wire-refresh journey tests with the existing `#[serial]`
  grouping so parallel-suite process-spawn contention on Windows no longer
  pushes them past their test deadlines (#8545).
