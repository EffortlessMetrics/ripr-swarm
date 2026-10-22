<!-- section: Internal / CI -->
- The shared `xtask` process runner now caps every drain that had no output budget at 256 MiB per stream; a child that floods a pipe fails with an error instead of being buffered without limit (#9480).
