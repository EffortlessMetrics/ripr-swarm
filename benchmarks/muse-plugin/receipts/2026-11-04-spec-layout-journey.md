# Plugin evaluation receipt — 2026-11-04 (specified layout, durable attempt)

Package: `plugins/muse/ripr` at candidate worktree state before the
reshape commit (plugin 0.1.0; manifest sha256
`14f43381e654ad6274d2aaad5bff41d80455d5b219dcb6f5261ae82c2d2c1018`,
package sha256
`61d7ecf2d6dddcd12a1fded3567fdf29cb0cd032ab3bde5c0b2c0d15280b41d4`).
Host: Muse Code 1.4.14, Windows. Installed fresh after removing
the earlier `dist/muse` registration (`muse plugins remove ripr`,
then install from `plugins/muse/ripr`); MCP capability
`plugin:ripr:mcp_server:core` approved explicitly and re-inspected as
`trusted_enabled` (definition hash
`sha256:d8f13e58a642238cb5d3132755b1c2be4f300186245f0f7adfa218d0dd9d7932`).
Validators before install: both skills and the plugin valid with empty
diagnostics. Binary: `ripr 0.11.0 (7fd1d0e00591d0b9cc18ce8a8f5a5130a037b7a3)`
(development build, identified as such; not a released-product claim).

Fixture: `benchmarks/muse-plugin/fixtures/rust-boundary-gap`
materialized as a two-commit git repo (`main` holds `>`, branch
`journey` holds `>=`), scenario `rust-boundary-gap`.

## Journey (fresh `muse exec` session, ripr-repair skill)

- Discovery: live `ripr_*` tools from server `core` in the new session.
- Before: snapshot
  `snapshot:sha256:108937c45cb4806de1fed72e95c891e083570596f6f8a5b396308fbe609d4201`,
  `fresh_for_working_tree: true`; gap
  `gap:rust:src/lib.rs:discounted_total:predicate_boundary:predicate:amount==discount_threshold`,
  class `weakly_exposed`, seam `67fc764ba37d77bd` (from the repair
  card's receipt binding), `repair_packet_ready: true`, limitation
  `no_limitation_established`.
- Durable attempt: first `--phase before` refused (`target/` not
  Git-ignored; nothing started), resolved through `.git/info/exclude`
  (not a tracked edit). Attempt `repair-attempt-0e453aadc3aeeb4a132fcffd`,
  allowed surface `tests/pricing.rs` only. `--phase after` exit 0,
  edit cage `Compliant`, static movement `weak -> exposed`
  (`weakly_gripped -> strongly_gripped`, improved); MCP receipt status
  `improved`, `terminal_receipt: issued`. The durable receipt itself
  records `Test run: none recorded` — static evidence only.
- Edit: exactly one test, `at_threshold_discounts` asserting
  `discounted_total(100, 100) == 90`; final Git diff touches only
  `tests/pricing.rs`.
- After (worktree refresh): snapshot
  `snapshot:sha256:0d571007677e13603caa5ce59878968a9338c286e99ffba470dcf5a203753ea2`,
  same item `exposed`, no missing discriminators.

Executed verdict (separate axis, agent-run `cargo test`): correct
behavior exit 0 (3 passed); wrong-implementation control (`>` restored)
exit 101 with `at_threshold_discounts` FAILED; restored exit 0.

Combined claim: `completed_repair` for scenario `rust-boundary-gap` —
static movement, executed discrimination, and a durable terminal
receipt agree, each in its own axis. Cost: 27 model calls, 1247867
input tokens (1134676 cached), 4586 output tokens. Wall time and peak
memory: not measured.

## Coverage against scenarios.toml

Executed end to end: `rust-boundary-gap` (this receipt),
`missing-or-unapproved-server` (prior receipt, earlier package bytes),
`stale-snapshot` (the worktree-refresh step above and the repair it
forced in the candidate).

The prior receipt's interior-value assertion passed under both behaviors
alongside an already-discriminating boundary test. That observation does
not establish an incorrect-only plugin end-to-end rejection run: the
overall wrong-implementation suite failed because the boundary test was
still present. `incorrect-repair-control` remains unexecuted as the
standalone scenario specified in `expected/incorrect-repair-control.json`.

Specified but not yet executed: `no-actionable-gap`,
`changed-head-or-baseline`, `test-only-edit-constraint` (its diff
oracle is incidentally satisfied by this run's test-only diff),
`typescript-weak-assertion` (retained regression by design).
Effectiveness cohorts (with/without conditions) remain exploratory
and unclaimed.
