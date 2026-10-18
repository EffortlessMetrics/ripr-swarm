<!-- section: Internal / CI -->
- Agent skills now require main-baseline triage before PR gate repair, local
  `cargo xtask ci-policy` proof for gate-relevant changes, a single-owner
  disposition for golden drift inherited from main, and PR head verification
  before push or merge (#8430, #8431, #8432).
