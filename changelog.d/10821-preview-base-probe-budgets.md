<!-- section: Fixed -->
- The empty-diff `--base` probe now screens admitted Python and
  TypeScript/JavaScript changed lines against the same preview budgets
  `ripr check` enforces, including C-quoted `handwritten_files` paths
  under excluded directories, so a recommended base cannot exceed a
  preview language the advised run would refuse (#10821).
