<!-- section: Internal / CI -->
- Windows MSVC dev and test builds now link with the toolchain-bundled
  `rust-lld`, and test artifacts carry no debuginfo
  (`[profile.test] debug = 0`, shrinking per-binary MSVC PDB output) (#8382).
