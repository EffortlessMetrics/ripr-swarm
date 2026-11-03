<!-- section: Fixed -->
- A doc example on an item gated by `#[cfg(feature = "..")]` (or an inner `#![cfg(feature = "..")]`) registers as a test again when the feature is in the package's default feature set, so default-feature crates such as rust-hex credit their doctests. `not(..)`, features outside the default set, target and custom cfg values, and `cfg_attr` still refuse (#12230, #8639).
