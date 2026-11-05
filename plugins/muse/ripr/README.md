# RIPR for Muse

Use RIPR in Muse to find one changed behavior that needs a stronger test,
inspect its evidence, and make a focused test repair. RIPR asks: for the
behavior changed in this diff, do the current tests appear to contain a
discriminator that would notice if that behavior were wrong?

This is the native Muse package for the Meta-plugin workstream. Its plugin
name is `ripr`; [the manifest](.muse-plugin/plugin.json) declares the package.
Plugin version **0.1.0** is independent of the RIPR CLI version. It connects
Muse to the existing analyzer and repair workflow, with no bundled binary,
hooks, reminders, or background daemon.

- [Install and check the connection](#install-and-check-the-connection)
- [Repair one gap](#repair-one-gap)
- [Choose the analyzed inputs](#choose-the-analyzed-inputs)
- [Troubleshooting](#troubleshooting)
- [Evidence and compatibility](#evidence-and-compatibility)

## What the package provides

| Component | Purpose |
| --- | --- |
| MCP server `core`, `ripr mcp --stdio` | Read workspace status, refresh static evidence, and inspect gaps, repair cards, and existing attempts. |
| [`ripr-setup` skill](skills/ripr-setup/SKILL.md) | Check the binary, plugin approval, tool discovery, startup root, and first refresh. |
| [`ripr-repair` skill](skills/ripr-repair/SKILL.md) | Guide one durable repair, from preparation before editing through separate static and executed checks. |

The package declares two skills and no separate command files. Use the
qualified skill names `/ripr:ripr-setup` and `/ripr:ripr-repair`; short forms
can be shadowed by another skill. A repository path supplied in a prompt
expresses intent; it does not switch the running MCP server's root.

The MCP server never edits source or runs tests. A refresh can write RIPR's
analysis cache under `target/ripr/`. Muse's file and shell tools perform any
authorized edits and test execution. Approving the MCP server does not grant
those tools additional authority.

## Install and check the connection

You need Muse, Git, a target Git repository, and an MCP-capable `ripr` binary
on the `PATH` inherited by Muse. The plugin does not download or install RIPR.
Check the binary before installing the plugin:

```sh
muse --version
ripr --version
ripr mcp --help
```

The [retained trials](#evidence-and-compatibility) used Muse Code **1.4.14**
and a pinned **RIPR 0.11.0** development build. RIPR **0.10.0** failed with
`unknown command "mcp"`. Check the command and tool discovery as well as the
version: a version number alone does not establish the required capabilities.
If the command is absent, install an
MCP-capable build through your normal installation route, then restart Muse
with that binary on its `PATH`.

Use a checkout containing `plugins/muse/ripr`. Replace the package path below
with its absolute path; this is the plugin's source directory, which can be
different from the repository you want to analyze.

```sh
muse plugins validate "/absolute/path/to/ripr-swarm/plugins/muse/ripr" --json
muse skills validate "/absolute/path/to/ripr-swarm/plugins/muse/ripr/skills/ripr-setup"
muse skills validate "/absolute/path/to/ripr-swarm/plugins/muse/ripr/skills/ripr-repair"
muse plugins install "/absolute/path/to/ripr-swarm/plugins/muse/ripr" --json
muse plugins inspect ripr --json
```

Validation should report `valid: true` with empty diagnostics. After install,
both skills can be active while the MCP server is still
`review_needed`. Inspect the declaration: it should launch only
`["ripr", "mcp", "--stdio"]`. When that is the server you intend to trust,
approve its capability and inspect the result:

```sh
muse plugins approve plugin:ripr:mcp_server:core --json
muse plugins inspect ripr --json
```

The recorded successful trial reported `trusted_enabled` for that capability.
Installation and approval are separate steps; neither alone demonstrates a
working analysis session. After a package update, inspect it again and approve
the current declaration if the capability is `review_needed`.

Start a **fresh Muse session from the target repository**, then run:

```text
/ripr:ripr-setup
```

The manifest does not pass `--root`. RIPR resolves the root once, when its
server starts, from the startup directory and repository markers. Changing a
shell's directory later, or passing a path to `/ripr:ripr-setup`, cannot retarget
the running server. Restart from the intended repository if the launch
context is wrong or cannot be established. Status returns root posture and a
hashed identity, not an absolute host path.

A working setup discovers the ten tools listed in the
[MCP reference](../../../docs/interop/mcp.md#what-it-exposes),
calls `ripr_workspace_status` with no arguments, and completes an explicit
refresh. The setup report names the input source, snapshot, freshness, and
any limitation. An empty diff is a scope result, not an all-clear.

## Repair one gap

Save the relevant source and test buffers, then ask Muse to repair one gap:

```text
/ripr:ripr-repair

Repair the gap at the discount threshold in tests/pricing.rs.
Use saved worktree inputs and base trunk on every refresh.
```

Use a test path and existing base from your repository; `trunk` is an example.
The [repair skill](skills/ripr-repair/SKILL.md) is the canonical workflow.
It inspects one bounded listing and one gap's evidence, checks the repair
card's readiness, and proposes a test that observes the changed behavior.
The ordinary edit-and-recheck path uses saved worktree inputs explicitly.

The guided repair prepares its durable CLI attempt **before the test edit**.
Require `card.readiness.repair_ready` to be `true`, obtain the real seam ID
from `card.subject.seam_id`, and prepare in the target repository:

```sh
ripr agent repair --root . --seam-id <seam-id> --phase before --json
# Preserve the attempt ID and exact next command printed by before.
# Make the one allowed test edit and run its authorized focused checks.
ripr agent repair --root . --attempt <repair-attempt-id> --phase after
```

The placeholders are different identities; do not substitute a gap's
`canonical_id` for the seam or attempt ID. A refused or ambiguous preparation
does not establish a usable baseline. Saved changes can be eligible for
worktree analysis while still inadmissible to a durable attempt: preparation
refuses tracked changes outside the selected test surface. Preserve those
changes and follow the returned recovery before editing. For Rust attempts,
Git must already ignore the entire `target/` directory. The ordinary after
phase accepts a focused test commit that descends from the prepared HEAD;
trust-bound Python
attempts have stricter identity and authorization requirements. See the
[repair-attempt contract](../../../docs/REPAIR_ATTEMPT.md)
for those conditions and recovery.

`ripr_prepare_repair` is an in-memory MCP transaction, not that durable CLI
before phase. Restarted sessions in the same repository can discover the
default durable store through `ripr_list_repair_attempts`, then read the
selected `attempt_id` with `ripr_get_repair_attempt` and the returned
`receipt_id` with `ripr_get_receipt_status`. Follow the returned continuation
and receipt routes; do not assume an arbitrary custom store is discoverable.

The final report keeps three observations separate:

| Observation | What to record |
| --- | --- |
| Static evidence | Before/after snapshot IDs, the same canonical gap, actual source/base, exposure class, readiness, and limitations. |
| Executed checks | The exact focused test command and result, plus whether that new discriminator fails against the wrong behavior in an isolated copy. |
| Durable receipt | The actual attempt ID, manifest, terminal state, receipt status, and retained artifact paths. |

Run a wrong-implementation control in a disposable fixture or worktree that
preserves the tested source and new test. Do not break the user's live
production tree. Confirm that the exact new discriminator fails for the
intended behavioral reason, then restore and rerun it in that isolated copy.
A compile error or an unrelated test failure is not the control result.
If execution is unavailable, say so; static improvement alone does not
establish a completed repair. The ordinary CLI after phase records static
evidence; it does not automatically record those separate test runs.
`--phase verify` is a separate trust-bound Python route, not generic Rust
test execution. Keep captured command output outside the repair repository.

## Choose the analyzed inputs

`ripr_refresh` chooses its input on **every call**:

```json
{"source": "worktree", "base": "origin/main"}
```

Use a base that exists in the target repository; `origin/main` is an example.
Retain the same explicit base on the before and after refreshes and inspect
the actual captured identity. Omitting `base` returns to the default branch;
omitting `source` returns to `committed`. Neither option is sticky.

| Input | Included |
| --- | --- |
| `committed` | The committed diff against the selected base. |
| `worktree` | That scope plus saved staged and unstaged tracked edits. |
| New, wholly untracked test file | Excluded until exact-path `git add -N -- <path>` is authorized and performed, or the file is otherwise tracked. Never stage unrelated work. |
| Unsaved editor buffer | Excluded; save it before refreshing. |

Bind gap/card reads to the returned `snapshot_id`. After an edit, request a
new worktree refresh and check the captured source, `fresh_for_working_tree`,
and any typed failure. A failed refresh can retain `last_known_good`; that
older snapshot is not after-edit evidence. If the gap cannot be read in the
new snapshot, report the reason instead of inferring that it closed.

## Troubleshooting

| Symptom | Next step |
| --- | --- |
| No `ripr_*` tools; server is `review_needed` | Inspect the installed capability and complete Muse's approval step. Start a fresh session. |
| No tools; `ripr mcp --help` fails | Check the binary on Muse's inherited `PATH`. The recorded 0.10.0 build predates MCP. Use an MCP-capable build and restart. |
| Tools refer to the wrong workspace | Check the session startup directory. Start a fresh session in the target repository; the no-argument status tool cannot change roots. |
| `analysis_failed` names missing base candidates | Choose an existing base, such as `trunk`, and pass it on every refresh. |
| Empty results | Read the outcome, scope counts, and `empty_scope_hint`. Confirm that the selected base/source contains the intended change. |
| A saved test edit is absent | Pass `source: "worktree"` again; check whether a new file is still untracked and whether the refresh failed. |
| `stale_snapshot` or a retained `last_known_good` snapshot | Follow the typed recovery route, obtain current evidence, and restart the dependent reads with its ID. |
| No ready gap, refused card, or `card.readiness.repair_ready` is not `true` | Report the limitation and stop the automated repair route. Do not invent a test target or upgrade readiness. |
| Durable preparation refuses the build-output rule | Establish an effective whole-directory `target/` ignore rule before preparing a new attempt; RIPR does not add it. |

## Evidence and compatibility

The [evaluation guide](../../../benchmarks/muse-plugin/README.md) separates
compatibility, repair behavior, and product effectiveness. Two retained
Markdown receipts describe Windows/Muse 1.4.14 runs with RIPR development
build `7fd1d0e00591d0b9cc18ce8a8f5a5130a037b7a3`:

| Historical receipt | Reported evidence | Limit |
| --- | --- | --- |
| [Cold install and repair](../../../benchmarks/muse-plugin/receipts/2026-11-04-cold-install-and-repair.md) | Installation, approval, tool discovery, and a boundary-test repair in the earlier `dist/muse` layout. | No durable CLI attempt ID; the interior-value example is not an isolated incorrect-only repair trial. |
| [Specified layout and durable attempt](../../../benchmarks/muse-plugin/receipts/2026-11-04-spec-layout-journey.md) | Fresh install with server `core`, one Rust boundary repair, a durable terminal receipt, and separate correct/wrong/restored test results. | Bound to the recorded package hashes; the static durable receipt itself records no test run. |

These are historical reported observations, not a fresh run of every scenario
or validation of later package revisions. The suite currently retains these
prose summaries and a run-record schema, not a committed machine-readable
`muse-plugin-eval-v1` run record. Repeated reproduction, comparative
with/without-plugin effectiveness, elapsed-time targets, peak memory, and
cross-agent plugin compatibility remain unestablished by these receipts.

Local development installation precedes marketplace/catalog distribution,
which is separate work. See the [host integration notes](../../../docs/interop/muse-code.md)
for package identities and session behavior. This native package does not
establish that an Agent Plugins MCP declaration imports in another host.

RIPR classes (`exposed`, `weakly_exposed`, `reachable_unrevealed`,
`no_static_path`, `infection_unknown`, `propagation_unknown`,
`static_unknown`) remain static evidence. They are never runtime mutation
outcomes.
