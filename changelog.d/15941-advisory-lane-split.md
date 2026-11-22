<!-- section: Changed -->
- The Windows Advisory nightly now splits each of its two test samples by
  target across parallel jobs (`windows-advisory-integration-*` for the
  integration targets, `windows-advisory` for unit targets, doctests and the
  verdict), and `xtask windows-advisory-summary` joins the parts
  (`--run1-part`, `--run2-part`) and checks them against the unsplit build's
  target list (`--expected-targets`), so the lane reaches a verdict inside its
  unchanged deadlines. macOS Advisory runs clippy and the library tests as
  separate jobs (#3922, #8391).
