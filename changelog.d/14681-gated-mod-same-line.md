<!-- section: Fixed -->
- Rust checks: a test under its owner's feature-gated `mod name;` declaration reads as an analyzer limit even when another `mod` shares that line (for example `#[cfg(feature = "v4")] mod v4; mod helpers;`), instead of keeping a false inert-assertion gap (#14681).
