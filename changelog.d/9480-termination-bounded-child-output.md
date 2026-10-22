<!-- section: Internal / CI -->
- The termination qualification harness reads each case's stdout and stderr through a 16 MiB per-stream budget; a binary that floods a stream fails its row as `instrument_failure` instead of being buffered without limit (#9480).
