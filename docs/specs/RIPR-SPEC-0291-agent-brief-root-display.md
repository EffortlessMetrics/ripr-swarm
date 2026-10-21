# RIPR-SPEC-0291: Agent brief start-command root display contract

Status: proposed

Owner: product / output

Created: 2026-10-21

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8995 (deferred from #8992 / #8994: the agent-brief start-command
  renderers shared the `bound_root(&display_path(root))` latent bug that
  rewrites a literal backslash in a Unix checkout path before `bound_root`
  sees the value)

Linked PRs:

- None yet

Support-tier impact:

- None. The agent brief JSON (schema `0.1`) keeps every field; only the
  rendered `next.*` and per-seam `verification.*` command strings change,
  and only for roots whose spelling `display_path` would rewrite.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- No new exposure class, oracle kind, severity, or stage-state value. The
  root-rendering rule is the shared #4287 contract already owned by
  `agent::loop_commands` (`root_path_display` before `bound_root`); this
  spec records the agent-brief surfaces' obligation to honor it, matching
  the CLI recovery route (`unknown_seam_id_hint`), the agent-stub lane
  (#8989) and the pilot surfaces (#8992, #8994).

## Problem

`output/agent_brief.rs` built its paste-able command lines with
`bound_root(&display_path(root))` — the same latent bug fixed on the
agent-stub and pilot surfaces. `display_path` rewrites backslashes before
`bound_root` sees the value, so a Unix checkout whose path contains a
literal backslash (e.g. `/tmp/team\repo`) got a pasted
`ripr check --root ...` / `ripr agent verify --root ...` command that
targets a different directory (`/tmp/team/repo`). Two renderers carried the
pattern: the brief-level `next` block (`inspect_packet`,
`verify_after_edit`) and each top seam's `verification` block
(`before_snapshot_command`, `after_snapshot_command`, `verify_command`).

## Behavior

- Every agent-brief start-command renderer renders the workspace root with
  `root_path_display(root)` before `bound_root`, so the root's literal
  characters survive: on Windows `\` is a separator and renders as `/`
  like every other command path; on Unix `\` is an ordinary filename
  character and renders exactly as the filesystem spells it (#4287).
- Ordinary roots are byte-identical to the previous rendering; the change
  is observable only for a root whose spelling `display_path` would
  rewrite.
- The redirect anchor (`prepare_workflow_command` via
  `anchored_redirect_target`) already follows the same rule, so the
  analyzed subject and the written artifact cannot name different
  repositories.

## Required Evidence

- A unit test renders the per-seam `verification` block for a plain string
  Unix root containing a literal backslash (`/tmp/team\repo`) and pins
  that `before_snapshot_command` and `verify_command` keep the backslash
  in the `--root` argument; on Windows the same test pins stable `/`
  separators.
- Existing agent-brief rendering assertions (relative roots, loaded-config
  paths, redirect anchoring) stay green unchanged.

## Test Mapping

- `crates/ripr/src/output/agent_brief.rs::tests::agent_brief_verification_preserves_a_literal_backslash_unix_root`
- `crates/ripr/src/output/agent_brief.rs::tests::agent_brief_json_renders_ranked_seam_summary`
  (redirect-anchor pin, unchanged)
- `crates/ripr/src/output/agent_brief.rs::tests::agent_brief_json_renders_loaded_config_and_empty_file_scope`
  (inspect-packet pin, unchanged)

## Acceptance Examples

Rendered on a Unix host with root `/tmp/team\repo` (excerpt):

```json
{
  "verification": {
    "before_snapshot_command": "mkdir -p '/tmp/team\\repo/target/ripr/workflow' && ripr check --root '/tmp/team\\repo' --mode draft --format repo-exposure-json > '/tmp/team\\repo/target/ripr/workflow/before.repo-exposure.json'",
    "verify_command": "ripr agent verify --root '/tmp/team\\repo' --before target/ripr/workflow/before.repo-exposure.json --after target/ripr/workflow/after.repo-exposure.json --json"
  }
}
```

The same rendering before this spec rewrote the root to
`/tmp/team/repo`, naming a directory that does not exist.

## Non-Goals

- The pilot surfaces (`output/pilot/commands.rs`) still carry the same
  pattern on main; they are dispositioned to the in-flight #8994 lane
  (RIPR-SPEC-0290) and are not changed here.
- No change to the brief's seam selection, omission vocabulary, schema
  version, or any non-command field (`root`, `config`, `working_set`,
  evidence blocks) — those keep `display_path` rendering, which is
  informational and never pasted as a shell command.
- No change to `bound_root`, `root_path_display`, or
  `anchored_redirect_target` themselves; the shared rule stays owned by
  `agent::loop_commands` (#4287).

## Implementation Mapping

- `crates/ripr/src/output/agent_brief.rs` owns both renderers:
  `render_agent_brief_json` (the brief-level `next` block) and
  `verification_json` (each top seam's `before_snapshot_command`,
  `after_snapshot_command`, and `verify_command`). Both now pass
  `root_path_display(root)` to `bound_root`; the informational
  `display_path` renderings elsewhere in the file are unchanged.

## Metrics

- `unit_test_pass_rate` on the registered tests.
