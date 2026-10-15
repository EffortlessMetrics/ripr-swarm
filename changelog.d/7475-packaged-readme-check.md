<!-- section: Internal / CI -->
- `cargo xtask check-packaged-readme` binds the README `cargo package -p ripr
  --list` would ship to the current-release sentence and the default-base
  cascade, so a stale crates.io storefront cannot cut undetected again (#7475).
