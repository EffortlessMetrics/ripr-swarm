<!-- section: Fixed -->
- A Rust helper wrapper that reads `env!` or `option_env!` before calling its helper now forwards the helper's result to tests of the wrapper, as it already did for `cfg!` and `line!`. A local `macro_rules! env` or `option_env`, a foreign import of either name, and a path-qualified call such as `std::env!` still keep the wrapper opaque (#10357).
