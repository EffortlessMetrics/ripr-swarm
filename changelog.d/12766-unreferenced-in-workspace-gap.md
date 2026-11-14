<!-- section: Changed -->
- Rust checks: a `no_static_path` finding on a function nothing in the workspace names now says `(unreferenced in workspace)` on its reach line, and the verdict corpus scores it as a gap instead of an abstention (abstention 0.383 to 0.367, #12766).
