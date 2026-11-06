<!-- section: Changed -->
- `ripr check` warm runs in TypeScript workspaces with many `export *`
  barrels no longer scan every re-export edge for every resolution frame:
  steady-state analysis on a 5,400-file barrel-heavy synthetic workspace
  dropped from ~409 s to ~65 s, with byte-identical findings. Findings,
  classifications, and all fail-closed chain-resolution semantics are
  unchanged; VS Code-shaped workspaces at current main were already fast
  here (relation index ~0.1–0.3 s), so this removes a barrel-shape quadratic
  rather than changing their profile (#11676).
