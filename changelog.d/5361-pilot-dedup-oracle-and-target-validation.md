<!-- section: Changed -->
- `ripr pilot` and repo-wide analysis rank each seam's related tests with one
  best-oracle computation per test instead of two, and read a shared test
  file's workspace fingerprint once per seam instead of once per test. Cold
  `ripr pilot` on this repository drops from 193 s to 160 s on a 4-core runner
  with byte-identical `repo-exposure.json` and `agent-seam-packets.json`
  (#5361).
