<!-- section: Internal / CI -->
- The packaged-install provenance check now has a test that runs a real
  `cargo install` and reads the install record Cargo writes. On Windows it also
  accepts a network-share package root (`file://server/share/...`), which it
  used to refuse (#10339).
