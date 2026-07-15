# Goal manifests

Campaign records are durable machine-readable context for `ripr` agents. They
describe objectives, work items, required proof, and claim boundaries without
selecting the repository's current issue, lane, branch, PR, worktree, or wave.

The migration compatibility pointer is:

```text
.ripr/goals/active.toml
```

It points at campaign records under:

```text
.ripr/goals/campaigns/
```

The pointer is non-authoritative and must not change merely because a root
selects a wave. Live execution state comes from the generated portfolio and
current GitHub/local evidence.

Archived or focused campaign records live under:

```text
.ripr/goals/archive/
```

## Ownership

Campaign records own:

- durable campaign identity and status;
- linked proposal, spec, ADR, and plan paths;
- machine-readable objectives and end-state checks;
- work-item IDs, statuses, dependencies, and proof commands;
- stable work-item identities and claim boundaries.

Goal manifests do not own:

- product rationale;
- behavior contracts;
- durable decisions;
- generated metrics or reports;
- support-tier claims;
- policy exceptions.

Move those to the linked proposal, spec, ADR, generated report, support-tier
row, or policy ledger.

## Agent boot order

Agents should read:

1. `AGENTS.md`.
2. `docs/REPO_TRACKING_MODEL.md`.
3. `docs/agent-context/CONTEXT_SYSTEM.md`.
4. The generated portfolio and current GitHub/local state.
5. The selected issue/work-item packet and relevant campaign record.
6. The linked implementation plan.
7. The linked spec and ADRs.

Then the root should select exactly one bounded issue/work item for each writer,
compile its packet, run the listed proof commands, and stop if linked artifacts
are missing or contradictory.

## Status values

Use these work-item statuses:

- `ready`: available to start.
- `active`: currently being worked.
- `blocked`: blocked by a named dependency or missing artifact.
- `done`: landed and evidence is recorded.
- `completed`: historical synonym used by some older docs; prefer `done` for
  new entries.
- `superseded`: replaced by another work item or lane.

Campaign records use `active` while their durable work is unfinished and
`closed` after their end state is recorded. A closed record should also have an
archive copy when it represents completed history. Campaign status is context;
it is not a repository-wide scheduler or current-wave marker.

## Validation

For manifest-only changes, run at minimum:

```bash
git diff --check
cargo xtask goals next --campaign <campaign-id>
cargo xtask check-goals
cargo xtask check-doc-index
cargo xtask check-pr-shape
```

Unqualified `cargo xtask goals next` intentionally returns migration guidance
and does not select a campaign. Use the live portfolio and selected issue
packet before starting work.

Also run any commands listed by the changed work item.
