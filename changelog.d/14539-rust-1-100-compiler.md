<!-- section: Internal / CI -->
- Build and qualify RIPR with Rust 1.100.0 while retaining the Rust 1.95 source
  minimum; explicitly select the minimum in its CI lane and keep coverage,
  compiler-layout diagnostics and developer setup compatible with both compiler
  generations (#14539).
