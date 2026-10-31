# Public command hierarchy

Choose the command for the task. This guide describes the development checkout;
see [installation](QUICKSTART.md#installation) for published and source builds.

| Task | Command | Result |
| --- | --- | --- |
| Inspect one change | `ripr check` | Static findings, or an explicit no-action or limited result. Analysis stages go to stderr; `--quiet` suppresses them. Machine stdout is unchanged. |
| Inspect a finding | The `ripr explain` command printed by `check` | Evidence for that finding, using the same root, diff, mode, and ID. |
| Hand off a finding | The `ripr context` command printed by `check` | Context for a human or coding agent. |
| Explore the repository | `ripr pilot --root .` | Optional broader analysis, pilot reports, and a supported next action. Not a prerequisite for `check` or `repair`. |
| Repair a selected Rust gap | `ripr repair` | A prepared before/edit/after attempt; you or your agent edit the test. |
| Resume a repair | `ripr status --root .` | The continuation command or a recovery step. |
| Compose PR evidence | `ripr first-pr` with the inputs described in [First PR workflow](FIRST_PR_WORKFLOW.md) | A summary of existing artifacts, not a new analysis or repair. |
| Add advisory CI | `ripr init --ci github` | A non-blocking GitHub workflow to review and commit. |
| Diagnose setup | `ripr doctor` | Tooling and configuration checks with recovery guidance. Not required before every run. |
| Record result usefulness | `ripr feedback record --snapshot ID --reason CODE` | Local receipt bound to a snapshot; diagnostics, classification, baselines, suppressions, gates, and gap closure stay unchanged. |
| Check configuration | `ripr config validate` | Validation of `ripr.toml` without analysis. |
| Start the LSP sidecar | `ripr lsp --stdio` | Saved-workspace feedback for an LSP client. |
| Serve MCP | `ripr mcp --stdio` | [Read-only workspace status and static analysis session](interop/mcp.md): an explicit `ripr_refresh` analyzes committed input; uncommitted input needs the opt-in `source: "worktree"`. No edit or project-test authority. |
| Read detailed help | `ripr help <command>`, `ripr help workflow [name]`, or `ripr help --all` | Options for one command, one bounded workflow's steps, or the full reference. |

The bare `cache`, `baseline`, `policy`, `reports`, `calibrate`, `zero`,
`assistant-loop`, `config`, `pr-ledger`, `pr-review`, `coverage-grip`, and `gate`
groups print their own help and exit successfully. For example, `ripr cache`
shows the same help as `ripr help cache`, without reading configuration or
writing reports. Choose a subcommand to perform an operation; its required
inputs still apply. `ripr gate --pr-guidance PATH` also accepts the flag-first
evaluation shorthand. `ripr pr-comments` retains its default `plan` action.

Mistyped `agent` subcommands list the accepted children and suggest a nearby
spelling when one exists: `ripr agent brief2` suggests `brief`. Suggestions
stay within the `agent` group. A distant miss such as `swarm`'s `ingest`
receives the accepted list without a suggestion. These errors exit 2, write
the recovery message to stderr, and leave stdout empty. Use `ripr agent --help`
or `ripr agent <subcommand> --help` to inspect the accepted command.

## Repair transaction

Start from `ripr check`. When it offers a `ripr repair` start, run the
command it prints. `ripr repair` accepts a seam ID, a `ripr check` finding ID
(`probe:` or `repo-probe:`), a canonical gap ID, or a `file:line` location, and
starts the before phase when the item maps to exactly one repair-eligible seam.
A finding ID the current seams no longer map, or that maps ambiguously, is
refused with a recovery route instead of a guessed seam. Without an item,
`ripr repair` starts only when exactly one eligible seam is visible; several
eligible seams print a selection list with a retry command, and none prints
setup or limitation guidance. IDs from different namespaces are not universally
interchangeable. Pilot is optional broader exploration; when it supplies a
supported repair it prints the seam ID and the equivalent advanced
`ripr agent repair` spelling.

The sequence below is a reference, not a copy-ready command: replace `ITEM`
with a seam ID or the check finding ID. When several attempts are current,
`ripr continue` names the explicit `--attempt` retry. Prefer the complete
commands printed by ripr.

```bash
ripr repair ITEM --root .
# Read the packet, then edit one allowed test and run its authorized test command.
ripr continue --root .
```

The repair command runs the before phase, which records the initial evidence
and prints the continuation command. The continue command runs the after
phase, which records static evidence after the edit and emits the receipt.
Test execution and static movement are separate observations.

Keep the printed continuation command when changing sessions, or rerun
`ripr continue --attempt ATTEMPT_ID --root .`. The seam ID selects the
gap; the attempt ID selects its prepared transaction. To recover, run
`ripr status --root .` and follow [Repair attempt identity](REPAIR_ATTEMPT.md).

The advanced compatibility form `ripr agent repair --seam-id ... --phase after`
requires exactly one waiting attempt for that seam. Zero or multiple matches
are rejected rather than guessed. The advanced spellings `ripr agent repair`
(`--phase before|after`) and `ripr agent status` run the same transaction with
explicit phase vocabulary.

### Trust-bound Python repair

Python uses a third, separately authorized verification phase. The following is
an argument reference; replace every placeholder with the value from the
accepted selection and prepared attempt.

```text
ripr agent repair --root . --seam-id <seam-id> --phase before --python-repair-trust-manifest <selection.json> --python-repair-trust-attempt <selection-attempt-id> --edit-authorized --edit-authority <operator-id>
# Edit one allowed test; retain the printed repair-attempt ID.
ripr agent repair --root . --attempt <repair-attempt-id> --phase after --edit-authorized --edit-authority <operator-id>
ripr agent repair --root . --attempt <repair-attempt-id> --phase verify --verify-authorized --verify-authority <operator-id>
```

`verify` accepts only `--attempt`, not a seam selector. The verification authority
must match the edit authority. Trust-selection flags belong on `before` only.
Optional `--verify-rollback` requests restoration after observation; inspect the
reported rollback result rather than assuming restoration.

Follow [the governed Python sequence](REPAIR_ATTEMPT.md#governed-python-sequence)
for selection, authorization, and recovery. Execution success does not itself
establish static improvement or acceptance.

## Advanced commands

`agent start`, `brief`, `card`, `packet`, `verify`, `verify-execute`, `receipt`, and
`review-summary` remain available for explicit control, compatibility, and
debugging. Use their help and the [LLM operator guide](LLM_OPERATOR_GUIDE.md) rather than
assembling them as mandatory first-run steps. `agent card` is the default
bounded handoff for one seam — the compact `RepairCardV1` (RIPR-SPEC-0194,
#4667); `agent packet` remains the compatibility and full-detail route behind
the card's explicit detail reference.

`feedback record` and `feedback export` record local usefulness judgments
against an immutable snapshot and join them onto existing route-quality rows.
They do not change diagnostics, classification, baselines, suppressions, gates,
or gap closure, and they do not open a network, editor, LSP, or MCP write
surface.

## Drift rule

README, Quickstart, editor onboarding, and CLI help should agree on each
command's job. Keep detailed options in command help and the relevant reference;
do not copy them into every introduction. The typed command-identity catalog lives in
`crates/ripr/src/cli/command_catalog.rs` ([RIPR-SPEC-0184](specs/RIPR-SPEC-0184-public-command-catalog.md),
[#4822](https://github.com/EffortlessMetrics/ripr-swarm/issues/4822)).
The richer discovery surfaces this guide once deferred are now shipped and
serve as its validation authority: the typed command metadata table validates
the human help and hierarchy documentation, and `help --all` exposes the
advanced and compatibility class markers
([RIPR-SPEC-0187](specs/RIPR-SPEC-0187-command-metadata-human-projection.md)),
`ripr help workflow` lists the bounded task workflows
([RIPR-SPEC-0189](specs/RIPR-SPEC-0189-workflow-catalog-help-workflow.md)), and
`help --json` emits the versioned machine-readable catalog
([RIPR-SPEC-0190](specs/RIPR-SPEC-0190-help-json-machine-discovery.md)).

## Non-claims

This guide describes existing commands. It does not authorize code edits or
verification, change support tiers, or turn static findings into runtime proof.
