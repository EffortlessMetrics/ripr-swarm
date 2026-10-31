<!-- section: Fixed -->
- TypeScript: the per-file fact store no longer writes one copy of a file's
  imports and scope bindings per test or owner. On VS Code the persisted
  facts shrink from 1.1 GB to 159 MB, peak memory from 1.47 GiB to 457 MiB,
  and the second `ripr check` is warm instead of the fourth (#11676).
