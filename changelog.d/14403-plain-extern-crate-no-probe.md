<!-- section: Fixed -->
- Rust checks: a private, plain `extern crate alloc;` line no longer becomes a finding (it only links a crate, like `use`); `pub extern crate x;` re-exports and `extern crate core as std;` are still checked (#14403).
