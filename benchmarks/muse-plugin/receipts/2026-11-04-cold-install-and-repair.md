# Plugin evaluation receipt — 2026-11-04 (cold install + one repair)

Host: Muse Code 1.4.14 (`Muse Code 1.4.14 (1.4.14-R7309.1)`),
Windows, plugin `ripr` 0.1.0 from candidate head `dbb837d7e9` (the
package then lived at `dist/muse` with one workflow skill, two
commands, and MCP server id `ripr`; the same candidate later adopted
the specified `plugins/muse/ripr` layout with `ripr-setup` /
`ripr-repair` skills and server id `core` — this receipt's install
and journey evidence binds the earlier package bytes, whose hashes
follow; the new layout's own validation and journey are recorded in
the PR evidence for the head that introduced it).
(package sha256 `c7d192877dd40a2dcf345836aefd3559d53964376ccce85cde0248499681f16d`,
manifest sha256 `2ecdf1a9719c31f296b2f2edaea30c20e2d51d0bfba3dd6cc1b64f524009dc2b`).
No pre-registered RIPR MCP server and no project `.mcp.json` RIPR entry
existed before installation (`muse plugins list` was empty).

## Compatibility layer

- `muse plugins validate dist/muse --json`: valid, diagnostics empty.
- `muse plugins install <dist/muse> --json`: installed, enabled,
  warning recorded verbatim: MCP servers require review before
  activation; skills and commands are active while enabled.
- `muse plugins inspect ripr --json`: skill and both commands
  effective; MCP candidate `plugin:ripr:mcp_server:ripr` started as
  `review_needed`.
- `muse plugins approve plugin:ripr:mcp_server:ripr --json`: enabled;
  re-inspection reports `trusted_enabled` with definition hash
  `sha256:d585607b06965ae94a63127454605b739df6cad11a4d785ce0886b3ea0809251`.

### Failed run (retained, not hidden)

First fresh session (`muse exec`, fixture below, PATH `ripr 0.10.0`):
skill and commands loaded from the installed plugin, but zero `ripr_*`
tools were callable. Direct probe: `ripr mcp --stdio` exited 2,
`ripr: unknown command "mcp"`. Root cause: released 0.10.0 predates the
MCP server, so the declared server never starts and no tools register.
The session stopped with that typed failure and made no edit. This run
is the reason `/ripr-setup`, the skill, and the README now state the
version floor explicitly. Usage: 12 model calls, 461533 input tokens
(424432 cached), 4113 output tokens.

### Passing run

Same fresh-session shape with an MCP-capable binary first on PATH
(`ripr 0.11.0 (7fd1d0e00591d0b9cc18ce8a8f5a5130a037b7a3)`): all ten
`ripr_*` tools discovered from the installed plugin's server, no
hand-written host configuration.

## Repair layer (positive case)

Fixture: `benchmarks/agentic/mcp-journey` identity
(`ripr-agentic-bench-manifest-v1`; base `>` on `main`, journey `>=`),
materialized as a two-commit git fixture on branch `journey`.

- Before: `ripr_refresh` outcome `complete_with_findings`, snapshot
  `snapshot:sha256:108937c45cb4806de1fed72e95c891e083570596f6f8a5b396308fbe609d4201`,
  `fresh_for_working_tree: true`. One repair-ready gap:
  `gap:rust:src/lib.rs:discounted_total:predicate_boundary:predicate:amount==discount_threshold`,
  class `weakly_exposed`, `src/lib.rs:2`, `repair_packet_ready: true`,
  top limitation `no_limitation_established`, seam `67fc764ba37d77bd`.
- Edit (agent, outside RIPR): one test added to `tests/pricing.rs`,
  `at_threshold_discounts` asserting `discounted_total(100, 100) == 90`.
- After: worktree refresh snapshot
  `snapshot:sha256:0f2b2144a8fa6917b2e7813ee08ddc5e5af46cf42ff5253bcd0c73ebf40ce69f`,
  same item now `exposed`, `missing_discriminators: []`.

Executed verdict (separate from the static verdict above):

- `cargo test` after the edit: exit 0, 3 passed.
- Wrong-implementation control (`>=` reverted to `>`): exit 101,
  `at_threshold_discounts` FAILED (`left: 100, right: 90`).
- Restored `>=`: exit 0, 3 passed. No production edit retained.

Combined claim: completed repair for this fixture — both the static
movement (`weakly_exposed` to `exposed` on the exact item) and the
executed discrimination support it. Cost for the passing session:
18 model calls, 747116 input tokens (662836 cached), 3825 output
tokens. Elapsed wall time and peak memory: not measured.

## Incorrect-repair control

In a scratch copy of the same fixture, a test asserting only the
interior value `discounted_total(101, 100) == 91` passes under both
behaviors: exit 0 on `>=`, and still passing on `>` in the same run
where `at_threshold_discounts` fails (exit 101). Such a test must not
be recorded as an improvement; the workflow's wrong-implementation
control is what exposes it.

## Not established

Agent effectiveness (with/without cohorts), repeated reproduction,
peak memory, elapsed-time targets, cross-agent compatibility, and any
dx-scoreboard baseline. Per the expansion rule in `evals/README.md`,
the corpus grows only after a second reproduced end-to-end result.
