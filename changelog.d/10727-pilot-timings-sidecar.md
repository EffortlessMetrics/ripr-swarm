<!-- section: Added -->
- `ripr pilot -v` also writes `pilot-timings.json` beside `pilot-summary.json`, with the same stages, elapsed milliseconds and item counts as the stderr table, for tools that should not parse it. A run without `-v` removes a prior sidecar, and the other artifacts are unchanged (#10727).
