<!-- section: Internal / CI -->
- Restrict the hosted Rust dependency cache to native Cargo profile directories,
  with a separate namespace to exclude older whole-target fixture and report
  archives. Workspace crates still compile for each candidate (#4987, #7685).
