<!-- section: Fixed -->
- Add the diff-scoped `Ripr Windows Gate Result` workflow (#5088): an always-reporting
  check that pass-as-skips when a pull request cannot touch `cfg(windows)` surfaces and
  otherwise runs the three-class Windows subset (gap artifacts, cache-warning smoke,
  ripr clippy) on `windows-latest`. Promotion to branch protection remains a separate
  owner step.
