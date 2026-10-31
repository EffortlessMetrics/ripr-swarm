<!-- section: Fixed -->
- Opening a very large unsaved Rust buffer in the editor sidecar no longer pays a quadratic walk to find type-bound declaration lines: the syntax tree and its tokens are indexed once per file instead of once per changed line (about 30% less analysis time on a 6,000-line buffer). Shutdown still waits for in-flight analysis; that is tracked separately (#11439).
