<!-- section: Fixed -->
- `ripr doctor` and the `ripr check` not-enabled disclosure now suggest
  `[languages] enabled` using languages actually present in the repo. A
  TypeScript-only tree is told to set `enabled = ["typescript"]` instead of
  dragging default `rust` into the copy-paste block; mixed Rust plus preview
  roots still keep both ([#7648](https://github.com/EffortlessMetrics/ripr-swarm/issues/7648)).
