<!-- section: Internal / CI -->
- `cargo xtask release-readiness` gains a `package-upload-size` check that
  measures the packaged `ripr` and `ripr-contract` crates against the crates.io
  default 10 MiB upload limit, which `cargo publish --dry-run` does not apply.
  An over-limit crate is reported as `warn` until the limit that applies to
  `ripr` is settled (#14071).
