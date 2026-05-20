# RIPR-SPEC-0059: Actionable Surface Translation

Status: proposed

## Problem

RIPR public projection now uses a more correct unit (`canonical_actionable_gap`),
but multiple user-facing surfaces still require too much prior context to act
safely.

A headline count like `184` is now plausible as repo repair queue size, but a
user still has to ask:

- what this number counts;
- what changed in the current PR;
- what to do in the current file;
- what packet is safe to hand to an external agent;
- what confirms that a repair attempt changed outcomes.

Without a strict translation contract between badge, PR, editor, swarm, and
outcomes surfaces, RIPR risks drifting back to report-volume presentation
instead of repair-loop guidance.

## Goal

Define a cross-surface translation contract so every first-screen surface answers
one top-level user question using the same actionable unit and explicit
non-claims.

## Non-Goals

- Changing badge count basis away from `canonical_actionable_gap`.
- Making new blocking CI gates by default.
- Expanding static findings vocabulary beyond existing conservative language.
- Turning editor or swarm surfaces into mutation proof or merge-readiness proof.

## Canonical User Questions

Each surface must lead with one question it answers first:

| Surface | First question | Primary unit |
| --- | --- | --- |
| Badge | How much actionable work remains in this repo? | unresolved actionable canonical gaps |
| PR evidence | What changed in this PR's repair queue? | actionable gap delta |
| Editor status | What should I do now in this workspace/file? | top safe actionable gap or fail-closed reason |
| Swarm attempt dry-run | What can an agent safely attempt right now? | one bounded repair packet |
| Outcome/trend report | What proved movement since last refresh? | receipt-linked outcome delta |

## Shared Unit and Terms

All surfaces in this spec must use:

- actionable canonical gap identity;
- repair route;
- related test or repair target;
- verify command;
- receipt command or state;
- advisory/static/preview boundaries;
- explicit non-claims.

Internal seam inventory and raw findings may be present only as supporting,
clearly labeled context below primary actionable sections.

## Surface Contracts

### 1) Badge and Badge-Basis Translation

Badge-adjacent user-visible copy must define the headline count at point of use:

```text
unresolved actionable static repair gaps
```

`ripr+` copy must state that additional test-efficiency items only count when
projected into the same repair/verify/receipt model.

Badge-basis output should include at least:

- public count;
- basis (`canonical_actionable_gap`);
- `ripr+` delta over base actionable count;
- separation from seam-native inventory;
- top exclusion reasons;
- last generated timestamp;
- trend versus previous refresh.

### 2) PR Evidence First-Screen

PR summary front panel should lead with actionable delta, not only repo
inventory:

- repo actionable queue count;
- PR-local actionable count;
- new actionable gaps;
- resolved actionable gaps;
- attempted-but-unchanged repairs;
- receipt missing/orphaned counts;
- blocked/static-limited counts;
- one top next repair packet with verify and receipt commands;
- non-claims.

Raw finding totals, seam inventory, or other diagnostics are secondary and
should appear below the actionable delta block.

### 3) Editor Show Status First-Screen

`Show Status` should lead with immediate repair clarity:

- workspace actionable count;
- current-file actionable/blocked/no-action counts;
- top repair item;
- related test/target;
- verify command;
- receipt command or state;
- next safe command when fail-closed.

If no safe action is available, the first-screen state must name the precise
blocking reason (for example stale artifact, wrong root, missing verify,
missing receipt path, unsupported schema) and a regeneration or setup command.

### 4) Swarm Attempt Packet Usability

`ripr-swarm attempt --dry-run` should emit a compact copy-ready operator/agent
packet section with:

- task;
- allowed files;
- do-not-change boundaries;
- repair target;
- verify command;
- receipt command;
- stop conditions;
- required return format.

This packet remains deny-by-default and must align with external-agent handoff
boundaries.

### 5) Outcome/Trend Movement

Outcome/trend front sections should answer motion since last refresh using
receipt-linked states:

- total actionable count;
- delta since prior refresh;
- resolved;
- improved;
- unchanged after attempt;
- orphaned/missing receipts;
- top blocked reason.

## Acceptance Criteria

This spec is satisfied when:

1. A new reader can describe what the badge count means without reading another
   spec.
2. A PR reviewer can identify next repair action from PR summary alone.
3. A developer in editor can see one safe repair action or one explicit
   fail-closed reason on first screen.
4. A swarm operator can copy one dry-run packet into an external agent without
   reconstructing missing boundaries.
5. Movement can be read as outcome delta, not just static queue size.

## Validation Signals

Validation should use existing guardrails plus focused evidence checks:

- `cargo xtask pr-summary`
- `cargo xtask check-pr`
- `cargo xtask lane1-evidence-audit`
- `cargo xtask actionable-gap-outcomes`
- `cargo xtask badge-basis`
- editor and swarm focused fixture/smoke checks for first-screen rendering

## Risks and Mitigations

- **Risk:** surfaces reintroduce raw findings as headline metrics.
  - **Mitigation:** enforce first-screen section ordering and non-claim checks in
    report writers and generated checks.
- **Risk:** same number appears with inconsistent labels across surfaces.
  - **Mitigation:** centralize shared phrasing constants for badge and front
    panels.
- **Risk:** packet verbosity blocks adoption.
  - **Mitigation:** define compact top blocks and push diagnostics below fold.

## Rollout Plan

1. Badge presentation pass (badge-adjacent meaning + basis link visibility).
2. PR summary first-screen actionable-delta contract.
3. Editor first-screen repair-first hierarchy polish.
4. Swarm dry-run packet copy-ready formatting alignment.
5. Outcome/trend movement front section.

Each step should remain a scoped PR with spec -> tests/fixtures -> code ->
output contract -> metric traceability.
