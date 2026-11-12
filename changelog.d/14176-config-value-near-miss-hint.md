<!-- section: Fixed -->
- A `ripr.toml` value that is an accepted spelling in another case, padded
  with spaces, or a small typo (`mode = "Draft"`, `enabled = ["Rust"]`,
  `exposed = "WARNING"`) now ends its load error with `(did you mean ...?)`
  naming the accepted value, as the `--mode` flag already did (#14176).
