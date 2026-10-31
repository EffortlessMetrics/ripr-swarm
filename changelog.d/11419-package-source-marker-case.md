<!-- section: Fixed -->
- Package-led JavaScript detection now classifies watcher paths under `src/`, `test/` or `tests/` with the host path rule, so a differently-cased directory (`Test/a.js`, `Src/a.js`) on Windows or macOS reloads the LSP configuration exactly as the detector sees it (#11419).
