<!-- section: Fixed -->
- Rust checks: a plain `extern crate alloc;` line no longer becomes a finding (it only links a crate, like `use`); `extern crate core as std;` is still checked (#14403).
