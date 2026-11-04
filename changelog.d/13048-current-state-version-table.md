<!-- section: Internal / CI -->
- `cargo xtask current-state` reads the crate version and MSRV from the `[workspace.package]` table instead of the first `version = ` / `rust-version = ` line anywhere in the workspace `Cargo.toml` (#13048).
