<!-- section: Internal / CI -->
- Windows MSVC dev and test builds now link with the toolchain-bundled
  `rust-lld`, test artifacts carry no debuginfo (`[profile.test] debug = 0`,
  shrinking per-binary MSVC PDB output), and test scratch space is
  confined to `target/test-tmp` instead of the `target/` root (#8382).
