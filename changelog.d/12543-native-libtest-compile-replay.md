<!-- section: Internal / CI -->
- `cargo xtask native-libtest` compiles one libtest target through Cargo JSON
  `--no-run`, binds the exact executable into a `ripr_native_libtest_build.v1`
  receipt, and replays that binary on the same worker without Cargo (#12543).
