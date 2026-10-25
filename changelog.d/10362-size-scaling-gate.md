<!-- section: Added -->
- Added `cargo xtask size-scaling --binary <path>`, which runs `ripr check` on generated Rust, Python and TypeScript files at 1x, 2x and 4x a base size and fails when cost grows faster than size, when the largest step exceeds a ceiling, or when a step ends abnormally (#10362).
