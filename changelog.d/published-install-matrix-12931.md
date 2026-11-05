<!-- section: Internal / CI -->
- Added the weekly, read-only `published-install-matrix.yml` lane. It installs the public crates.io, PyPI and npm packages on Linux x86-64/ARM64, macOS ARM64/Intel and Windows, starts each in a fresh Cargo project, and reports documented unavailable cells as known gaps (#12931).
