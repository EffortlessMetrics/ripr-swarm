<!-- section: Fixed -->
- Rust: the known-comparison-macro exception for a boolean `assert!` in the return-value family now requires the macro to be a direct item of its crate (`use float_cmp::approx_eq` or `use float_cmp::{.., approx_eq}`); a deeper path such as `float_cmp::other::approx_eq` no longer counts (#10389).
