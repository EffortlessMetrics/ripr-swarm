<!-- section: Internal / CI -->
- The `ripr` crate is now packaged and published from a publish stage whose
  source has its inline `#[cfg(test)]` modules removed, so the `.crate` fits
  under crates.io's 10 MiB upload limit with room to grow. `cargo xtask
  publish-dry-run` builds, packages and measures the stage and leaves it at
  `target/publish-stage` for the real publish; the checkout keeps every test
  (#14198).
