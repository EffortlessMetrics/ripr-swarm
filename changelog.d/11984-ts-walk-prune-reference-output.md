<!-- section: Fixed -->
- TypeScript workspace discovery and the TypeScript/JavaScript diff loop
  prune `baselines/reference`, `baselines/local`, and `__snapshots__`
  trees, so a large compiler clone does not spend the walk budget on
  reference output before `src/` is indexed. A bare `baselines` directory
  still counts as ordinary source. The existing generated/excluded-path
  limitation names skipped changed files under those trees (#11984).
