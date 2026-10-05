<!-- section: Changed -->
- The install-time notes now separate the two source routes. A workspace build
  (the git install in the README) uses the workspace release profile
  (`lto = true`, `codegen-units = 1`) and took 12.5 minutes on a 4-core
  container; the packaged 0.11.0 crate carries no profile section, so a
  crates.io install builds with cargo's default release profile and took 4.6
  minutes there. About 700 of 907 CPU-seconds are the `ripr` crate itself, not
  its dependencies (#5311).
