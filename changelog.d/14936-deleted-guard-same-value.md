<!-- section: Fixed -->
- Rust: a deleted early return (`if x == 0 { return Vec::new(); }`) whose value the remaining constant tail also returns no longer reads `exposed`; an exact pin on the guard's input cannot tell the two versions apart, so the finding reads `weakly_exposed` (#14936).
