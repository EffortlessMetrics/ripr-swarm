<!-- section: Internal / CI -->
- The release-readiness packaged-install check now requires Cargo's install
  record to name the extracted package root as the source of the installed
  `ripr`. The earlier digest comparison passed for any debug-vs-release pair,
  including an install from the workspace path at the same clean commit (#9312).
