<!-- section: Internal / CI -->
- The published `ripr` crate no longer carries test-only sources (out-of-line
  `#[cfg(test)]` modules and all of `tests/` except the `cli_help_hierarchy`
  target), which brings the packaged crate from 14.3 MB to 10.0 MB, under the
  crates.io 10 MiB upload limit. `cargo xtask release-readiness` gains a
  `package-upload-size` check that fails on a packaged crate over that limit
  and warns within 10% of it; `cargo publish --dry-run` does not apply the
  limit (#14071).
