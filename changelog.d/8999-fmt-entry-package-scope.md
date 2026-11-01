<!-- section: Fixed -->
- Rust: a test in a dependent workspace member that formats a root-package type no longer reaches a changed helper called only by that type's `Display`/`Debug` `fmt`; the helper-entry route now uses the same in-package rule as the inherent method entry (#8999, RIPR-SPEC-0297).
