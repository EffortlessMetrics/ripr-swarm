<!-- section: Fixed -->
- Opening a very large unsaved Rust buffer in the editor sidecar no longer re-walks the whole syntax tree and token stream for every changed line when finding type-bound declaration lines: both are indexed once per file (about 30% less analysis time on a 6,000-line buffer). Shutdown still waits for in-flight analysis; that is tracked separately (#11439).
