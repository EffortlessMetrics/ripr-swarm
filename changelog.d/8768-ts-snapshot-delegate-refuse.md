<!-- section: Fixed -->
- TypeScript committed-snapshot credit now refuses a Jest/Vitest runner
  config that names `projects`, `workspace`, or `root` (those files are
  not walked), a `snapshotFormat` option, or a whitespace-prefixed `-u`
  update flag, so those shapes stay `snapshot` / medium instead of
  promoting (#8768).
