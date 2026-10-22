<!-- section: Internal / CI -->
- `cargo xtask termination-removal-builds` scripts the repairs-disabled builds
  for the termination qualification harness and writes a manifest; the harness
  verifies it with `--removal-manifest`, refuses a duplicate `--removal-control`,
  runs each control from an owned copy, takes `--expected-commit`, and adds
  four execution-boundary controls (#4510, #9480). No product behavior changes.
