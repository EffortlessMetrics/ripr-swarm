<!-- section: Fixed -->
- Rust checks: a changed const named by a same-file compile-time assertion, and a changed continuation line of a multi-line `const _: () = assert!(..)`, report `compile_time_invariant` instead of `no_changed_rust_line` or `no_static_path` (#12040).
