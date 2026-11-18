<!-- section: Internal / CI -->
- Verdict corpus: add a local `cargo xtask verdict-corpus check-affected`
  selector that checks only the corpora affected by the supplied changed
  paths, widening fail-closed to all corpora on shared or unknown surfaces
  and naming unselected corpora as not run; the required `check-all` gate is
  unchanged (#14644).
