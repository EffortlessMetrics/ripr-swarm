<!-- section: Added -->
- `cargo xtask first-run --install-archive VERSION` walks the prebuilt release
  archive: it downloads the host's server archive, verifies its SHA-256
  sidecar, extracts `ripr` into the walk's own directory and runs that
  executable, recording the route, asset, digest and acquisition time apart
  from the source-install sample (#5311).
