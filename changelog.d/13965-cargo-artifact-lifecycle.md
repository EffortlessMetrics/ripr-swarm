<!-- section: Internal / CI -->
- Agent checkouts now disable low-reuse Cargo incremental state by default and
  provide receipted exact-target cleanup that preserves `target/ripr` without
  sharing or age-collecting another workspace's build tree (#13965).
