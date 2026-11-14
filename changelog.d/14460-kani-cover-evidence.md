<!-- section: Internal / CI -->
- `cargo xtask kani` now requires positive, complete cover evidence before reporting a harness as verified; missing, ambiguous or undetermined evidence remains unestablished, and four constant-only harnesses now carry explicit reachability witnesses (#14460).
