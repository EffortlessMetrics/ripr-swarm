<!-- section: Changed -->
- `ripr check --help` now tells agents and scripts to pair `--json` with `--quiet`
  (`--json` alone still writes `ripr progress:` lines to stderr, per the progress
  contract) and points at `summary`, `canonical_next_action` and
  `ripr context --at <finding-id> --json` as the lean way to read the large
  report; the quickstart says the same (#11139).
