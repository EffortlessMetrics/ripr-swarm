<!-- section: Changed -->
- `ripr check --help` now shows `--json --quiet` in its example and says
  `--json` alone still writes `ripr progress:` lines to stderr; the full help
  tells agents and scripts to pass `--quiet`, and the quickstart points at
  `summary`, `canonical_next_action` and
  `ripr context --at <finding-id> --json --quiet` as the lean way to read the
  large report (#11139).
