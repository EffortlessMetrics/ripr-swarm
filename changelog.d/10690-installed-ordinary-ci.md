<!-- section: Internal / CI -->
- Python wheel qualification runs the existing ordinary-test journey against
  the identified pip-installed analyzer after Rust-free installation, with a
  separately staged source-bound test runner, pinned native-test toolchain and
  retained identity, command and outcome evidence (#10690).
