<!-- section: Internal / CI -->
- `crates/ripr/Cargo.toml` no longer declares `bytes` as a direct dependency.
  No first-party code names the crate: the MCP transport reaches `BytesMut`
  through `tokio_util::bytes`, and SCIP admission decodes from byte slices. The
  resolved graph is unchanged because `prost` and `tokio` still require it
  (#11475).
