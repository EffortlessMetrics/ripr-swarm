<!-- section: Added -->
- `ripr pilot --format json` prints the existing `pilot-summary.json` document
  on stdout and suppresses the human terminal, so CI can consume the packet
  summary without reading the out-dir. `--json` is not an alias (#7428).
