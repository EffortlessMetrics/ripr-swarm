# Handoff: Actionable Surface Translation Closeout

Date: 2026-05-23

Branch: `campaign-actionable-surface-translation-closeout`

Current work item: `campaign/actionable-surface-translation-closeout`

Closeout ID: `RIPR-CLOSEOUT-0059`

Linked proposal: [RIPR-PROP-0016](../proposals/RIPR-PROP-0016-actionable-surface-translation.md)

Linked spec: [RIPR-SPEC-0059](../specs/RIPR-SPEC-0059-actionable-surface-translation.md)

Linked plan: [RIPR-PLAN-0059](../../plans/actionable-surface-translation/implementation-plan.md)

Archived manifest:
[`.ripr/goals/archive/2026-05-23-actionable-surface-translation.toml`](../../.ripr/goals/archive/2026-05-23-actionable-surface-translation.toml)

## Current State

Actionable Surface Translation is closed. The campaign made the covered
first-screen surfaces translate existing actionable canonical gap evidence into
the same repair-first question:

```text
what actionable gap remains,
what repair route is safe,
what verifies movement,
what receipt records it,
and what remains advisory or static-only?
```

The campaign did not change analyzer truth, actionable-gap producer schema,
default CI blocking, gate authority, PR comment publishing, source editing,
generated tests, provider/model calls, mutation execution, release behavior, or
badge endpoint values. It stayed presentation and projection focused: typed
fields decide, prose explains, and raw findings or seam-native inventory remain
supporting context below the actionable unit.

`.ripr/goals/active.toml` now records `status = "closed"` and
`no_current_goal = true`. A future worker should select the next campaign from
repo-owned state, not continue this closed lane from chat context.

## Prompt-To-Artifact Audit

| Requirement | Artifact | Validation |
| --- | --- | --- |
| Source-of-truth stack exists | #309 added `RIPR-PROP-0016`, linked `RIPR-SPEC-0059` to `RIPR-PLAN-0059`, registered the artifacts, updated indexes, and selected the active manifest. | `cargo xtask check-doc-artifacts`, `cargo xtask check-goals`, `cargo xtask goals next`, `cargo xtask pr-body --work-item docs/actionable-surface-translation-stack`, `cargo xtask repo-contract-report`, docs checks, `cargo xtask check-pr` |
| Badge/badge-basis copy explains the public count | #314 made badge-adjacent copy and badge-basis output define the headline as unresolved actionable static repair gaps using `canonical_actionable_gap`. | `cargo xtask badge-basis`, `cargo xtask check-badge-diff-policy`, `cargo xtask check-output-contracts`, `cargo xtask check-static-language`, `cargo xtask check-pr` |
| PR evidence leads with actionable delta | #315 made PR summaries lead with repo actionable count, PR-local actionable count, new/resolved gaps, receipt state, blocked/static-limited counts, and one top next repair packet before raw path inventory. | `cargo xtask pr-summary`, `cargo xtask check-output-contracts`, `cargo xtask check-pr` |
| Editor status leads with repair clarity | #316 made `ripr: Show Status` start with a repair cockpit block naming workspace/current-file actionable state, top repair, related proof, verify command, receipt state, and fail-closed next action. | `cargo test -p ripr lsp --lib`, `npm --prefix editors/vscode run compile`, `npm --prefix editors/vscode run test:e2e`, `cargo xtask lsp-cockpit-report`, `cargo xtask check-output-contracts`, `cargo xtask check-pr` |
| Swarm dry-run emits a copy-ready packet | #317 made `ripr-swarm attempt --dry-run` start with a compact operator packet containing task, allowed files, do-not-change boundaries, repair target, verify command, receipt command, stop conditions, and return format. | `cargo test -p xtask ripr_swarm_attempt --bin xtask`, `cargo xtask check-output-contracts`, `cargo xtask check-pr` |
| Outcome/trend reports lead with receipt-linked movement | #320 made actionable-gap outcomes lead with current actionable count, receipt-linked delta, resolved, improved, unchanged after attempt, missing/orphaned receipts, and top blocked reason. | `cargo xtask actionable-gap-outcomes`, `cargo xtask check-output-contracts`, `cargo xtask check-pr` |
| Active goal is closed and archived | This closeout marks the final work item done, accepts the proposal/spec, marks the plan done, registers `RIPR-CLOSEOUT-0059`, archives the manifest, and records this handoff. | `cargo xtask closeout --goal actionable-surface-translation`, closeout validation |

## PR Chain

- #309 `docs: activate actionable surface translation`
- #314 `badge: clarify actionable basis presentation`
- #315 `report: lead pr summary with actionable delta`
- #316 `vscode: lead status with repair state`
- #317 `swarm: make dry-run packets copy-ready`
- #320 `outcome: lead with movement front section`
- `campaign: close actionable surface translation`

## Closeout Validation

Closeout validation for this PR:

```bash
cargo xtask check-doc-artifacts
cargo xtask check-goals
cargo xtask check-doc-index
cargo xtask markdown-links
cargo xtask check-static-language
cargo xtask check-doc-roles
cargo xtask check-traceability
cargo xtask check-capabilities
cargo xtask check-support-tiers
cargo xtask check-pr
git diff --check
```

The behavior slices also passed focused proof while landing:

```bash
cargo xtask badge-basis
cargo xtask pr-summary
cargo test -p ripr lsp --lib
npm --prefix editors/vscode run compile
npm --prefix editors/vscode run test:e2e
cargo xtask lsp-cockpit-report
cargo test -p xtask ripr_swarm_attempt --bin xtask
cargo xtask actionable-gap-outcomes
cargo xtask check-output-contracts
```

#320 initially saw one hosted VS Code e2e clipboard timeout in
`startCurrentRepair executes the nearest existing repair action`. The same PR
head passed `cargo xtask vscode-test-e2e` locally, and the GitHub Actions rerun
passed the VS Code job without code changes. The failure is recorded as a CI
clipboard flake, not a campaign blocker.

## Claim And Support-Tier Changes

No support-tier promotion landed. `RIPR-PROP-0016` and `RIPR-SPEC-0059` are now
accepted because the covered surfaces implemented the translation contract, but
the support claim remains advisory/static.

The accepted claim is narrow:

- badge, PR, editor, swarm, and outcome first screens now lead with the relevant
  actionable canonical gap, actionable delta, bounded packet, or
  receipt-linked movement unit;
- the surfaces repeat the advisory/static boundary and keep runtime,
  coverage, mutation, policy, gate, merge-readiness, source-edit,
  generated-test, and provider claims out of scope;
- receipts remain static movement evidence, not mutation proof or coverage
  adequacy.

## Policy Ledger Changes

`policy/doc-artifacts.toml` now records:

- `RIPR-PROP-0016` as accepted;
- `RIPR-SPEC-0059` as accepted;
- `RIPR-PLAN-0059` as done;
- `RIPR-CLOSEOUT-0059` as done.

No CI, lint, file, package, no-panic, network, release, or branch-protection
policy changed.

## Remaining Limits

- Static evidence does not imply runtime adequacy, coverage adequacy, mutation
  confirmation, general correctness, merge approval, or policy eligibility.
- Generated CI remains advisory unless a separate policy decision changes gate
  behavior.
- Badge endpoint values remain generated-only and were not refreshed by this
  closeout.
- PR comment publishing remains unchanged.
- Editor surfaces remain read-only and projection-only.
- Swarm dry-run packets are copy-ready handoff packets, not autonomous repair
  execution.
- Outcome movement is receipt-linked static movement, not runtime proof.
- Preview language evidence remains advisory and static-limit bounded.

## Artifacts

- `.ripr/goals/active.toml`
- `.ripr/goals/archive/2026-05-23-actionable-surface-translation.toml`
- `docs/handoffs/2026-05-23-actionable-surface-translation-closeout.md`
- `docs/proposals/RIPR-PROP-0016-actionable-surface-translation.md`
- `docs/specs/RIPR-SPEC-0059-actionable-surface-translation.md`
- `plans/actionable-surface-translation/implementation-plan.md`
- `policy/doc-artifacts.toml`
- `.ripr/traceability.toml`

## Next Recommended Goal

No successor campaign is selected. Inspect repo-owned state in this order:

1. open pull requests and required checks;
2. `cargo xtask goals next`;
3. `docs/IMPLEMENTATION_CAMPAIGNS.md`;
4. `docs/IMPLEMENTATION_PLAN.md`;
5. accepted proposals, specs, ADRs, plans, and open issues that cite them.

## What Not To Do

- Do not reopen Actionable Surface Translation for new analyzer behavior.
- Do not treat first-screen translation as gate, policy, merge, coverage,
  runtime, mutation, or correctness authority.
- Do not refresh badge endpoint values in ordinary campaign closeout work.
- Do not add source edits, generated tests, provider/model calls, mutation
  execution, PR comment publishing, release behavior, or branch-protection
  changes to this closed campaign.
