# RIPR-SPEC-0290: Pilot repair start root display preserves literal backslashes

Status: proposed

Owner: product / output

Created: 2026-10-20

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8992 (offered_repair_start_route rewrites Unix roots containing a literal
  backslash)
- #4287 (root_path_display: render a selected root without rewriting its
  characters)

Linked PRs:

- None yet

Support-tier impact:

- No tier change. The pilot's offered repair start route, first-pr start
  command, and retry/snapshot/outcome paths are human copy/paste guidance; the
  native path rendering rule is internal command hygiene, not a new oracle kind,
  strength, class, relation reason or limitation kind.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No schema version bump. No new wire shape: the pilot summary JSON, Markdown
  and terminal renderers emit the same fields; only rendered roots or artifact
  paths inside command strings change, and only for paths containing a
  literal backslash on a non-Windows host.

## Problem

`offered_repair_start_route` in `crates/ripr/src/output/pilot/commands.rs`
built its advertised `ripr agent repair --root ...` start command with
`bound_root(&display_path(root))`. `display_path` replaces every backslash
with a path separator before `bound_root` sees the value, so for a Unix
checkout whose path contains a literal backslash (e.g. `/tmp/team\repo`) the
pasted command targets a different directory and cannot repair the selected
seam. The same latent pattern sat in two sibling pilot renderers in the same
module: `python_card_first_pr_command` (the `ripr first-pr --root ...` start
command for a Python repair card) and the `PilotCommands` retry/snapshot
command root.

The fix pattern already exists in-tree: `root_path_display` renders a
selected root without rewriting its characters — on Windows `\` is a
separator and renders as `/`, like every other command path; on Unix `\` is
an ordinary filename character and the root renders exactly as the
filesystem spells it (#4287). The CLI repair recovery path
(`unknown_seam_id_hint`) and the agent-stub `next_repair` surface (#8989)
already bind the root this way; PR #8989 deliberately deferred the pilot
surface because it is golden-pinned across pilot/LSP surfaces.

## Behavior

### Rule 1: repair start route root rendering

`offered_repair_start_route` renders the workspace root with
`root_path_display(root)` before passing it to `bound_root`. `bound_root`
then resolves a relative root once against the producing process working
directory and renders the absolute result with `root_path_display` again, so
the command line names the repository the pilot run analyzed:

- On Windows the rendered root keeps stable `/` separators, exactly as
  before; roots without a literal backslash are byte-identical to the old
  output.
- On Unix a root containing a literal backslash (e.g. `/tmp/team\repo`)
  keeps that backslash, so the pasted command targets the user's actual
  checkout.

### Rule 2: sibling pilot start-command renderers

`python_card_first_pr_command` and the `PilotCommands` retry/snapshot
command root follow the same rule in the same module: the root reaches
`bound_root` through `root_path_display`, never through `display_path`.
One root rule for every pilot paste command keeps the rendered family
consistent, so a projection consuming one command cannot disagree with the
screen that printed another.

### Rule 3: no behavior change for ordinary roots

For roots and artifact paths without a literal backslash, and on Windows for
all paths, the rendered commands are unchanged: `root_path_display` equals
`display_path` there. No golden, fixture pin, or human output moves.

### Rule 4: native artifact paths

The retry `--out`, after-snapshot redirect, and outcome `--before` and
`--after` arguments use `root_path_display` for their bound artifact paths,
then the existing shell argument quoting. Default output under the selected
root keeps its literal Unix backslashes and quotes. Pasting the commands
from another working directory must read and write the original packet,
leaving a separately existing slash-normalized sibling untouched.

## Required Evidence

- `pilot_followups_preserve_native_default_artifacts` independently pins all
  retry/snapshot/outcome paths and the Python first-pr route for native roots,
  an apostrophe plus backslash root, and an ordinary-root control. It rejects
  the pre-repair artifact rendering.
- Real CLI execution of default-output retry, snapshot, and outcome commands
  from a foreign CWD checks expected exits, native artifact identity, source
  custody, and slash-sibling non-mutation. Removing the native before input
  must make the same outcome command fail.

- A unit test pins `repair_start_command` for a Unix root containing a
  literal backslash (`/tmp/team\repo`) and asserts the offered route keeps
  the backslash (`--root '/tmp/team\repo'`), with a Windows assertion for
  stable separators; no process spawning.
- The existing pilot repair-line tests (canonical decision, terminal/JSON/
  Markdown offer rows, negative control) stay green, covering Rule 3.

## Non-Goals

- The agent-stub `next_repair` surface and the CLI repair recovery path:
  already fixed under #8989 / #4287 respectively; this spec covers the
  pilot module only.
- `output/agent_brief.rs`, which still renders two command roots through
  `display_path`; a different module and surface, left for its own bounded
  lane.
- The pilot summary's non-command path fields (the `root` field of the
  canonical next-action input keeps `display_path`, matching every other
  producer's root field); only command strings a user pastes are in scope.
- No LSP protocol change; no new wire shape.

## Acceptance Examples

1. Pilot runs with root `/tmp/team\repo` on Unix and the top seam is
   route-ready: the terminal, Markdown, and JSON repair command all read
   `ripr agent repair --root '/tmp/team\repo' --seam-id <id> --phase before`.
2. Same run with root `.`: the repair command uses the absolute producer-CWD
   root, unchanged, with the existing shell quoting and seam ID.
3. A Python repair card on the same literal-backslash root offers
   `ripr first-pr --root '/tmp/team\repo'`.
4. The pilot retry suggestion on that root names the same literal root.

## Test Mapping

- `crates/ripr/src/output/pilot/tests/native_artifacts.rs`:
  `pilot_followups_preserve_native_default_artifacts` (examples 3 and 4,
  Rule 4, and ordinary-root control).

- `crates/ripr/src/output/pilot/tests.rs`:
  `pilot_repair_start_preserves_a_literal_backslash_unix_root` (example 1,
  with the Windows stable-separator branch).
- `crates/ripr/src/output/pilot/tests.rs`:
  `pilot_repair_line_follows_the_canonical_pilot_decision` (example 2's
  ordinary-root shape and the no-repair negative control).

## Implementation Mapping

- `crates/ripr/src/output/pilot/commands.rs`:
  `offered_repair_start_route`, `python_card_first_pr_command`,
  `PilotCommands::new` (command root), rendering the root with
  `crate::agent::loop_commands::root_path_display` before
  `crate::agent::loop_commands::bound_root`.
- `PilotCommands::new` binds artifact `Path` values first with
  `bound_root_path`, then renders them through `root_path_display` and the
  existing command builders or `shell_arg`.
- `crates/ripr/src/agent/loop_commands.rs`: `root_path_display`,
  `bound_root` (unchanged; the shared rule owner, #4287).

## Metrics

- `unit_test_pass_rate`: the pilot repair-line tests above are required
  Rust tests; a failure blocks merge.
