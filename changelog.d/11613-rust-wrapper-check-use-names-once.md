<!-- section: Changed -->
- The Rust macro-binding scan collects each file's use-imported names once instead of walking every `use` tree for every macro call it checks for a local wrapper. On the `ripr-swarm-2026-10` scoreboard checkout a warm draft check drops from 81.5 s to 73.6 s wall (median of 3, 4 CPUs) with byte-identical findings (#11613).
