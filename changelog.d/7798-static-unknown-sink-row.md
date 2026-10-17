<!-- section: Fixed -->
- The flow-sink evidence row no longer renders tautologically when the sink text restates the kind label: a `static_unknown` finding now shows `local flow reaches unknown sink (line N)` instead of `unknown sink: unknown sink`, on both the human explain surface and the machine JSON `evidence_path`. Sinks with distinct text keep the `kind: text` form unchanged (#7798).
