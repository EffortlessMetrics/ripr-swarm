<!-- section: Fixed -->
- Failure-only coverage diagnostics retain exact warning paths containing Unicode
  control characters, report bounded-prefix UTF-8 validity and unavailable source
  lists explicitly, use target ABI file-open flags, and require matching LLVM
  prerequisites for the Linux replay test. Linux workspace and coverage CI
  routes prepare the matching LLVM component on pinned Rust 1.95.0 (#7492).
