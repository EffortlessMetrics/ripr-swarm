<!-- section: Internal / CI -->
- Hosted Rust qualification uses two complete hash partitions instead of four,
  avoiding two workspace test builds while retaining the full test inventory,
  separate doctests and complete, disjoint result verification (#8194).
