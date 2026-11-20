<!-- section: Changed -->
- `ripr pilot` spends less time on large Rust workspaces. Derive-name macro
  detection now scans each file once instead of once per derive attribute, and
  free-function name lookups read a per-index bucket instead of scanning every
  workspace function. On a cold release pilot of a 15-file edit in bevy the
  run went from 51-62 s to 32-35 s (two paired runs; polars 40 s to 38 s),
  with `repo-exposure.json`, `repo-exposure.md` and `agent-seam-packets.json`
  byte-identical (#13818, #13828).
