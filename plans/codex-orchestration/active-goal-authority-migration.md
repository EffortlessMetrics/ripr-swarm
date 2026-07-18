# Active-goal authority migration audit

Status: accepted prerequisite contract for #1697

## Scope

This plan inventories and classifies singleton active-goal consumers before
#1701 changes authority. It does not move files, choose a campaign, or grant a
writer permission.

## Reviewed slices

1. Inventory tracked readers, documentation, policy, fixtures, workflows, and
   captured issue contracts. Unknown paths remain `not_proven` and block the
   migration.
2. Freeze per-consumer target classification, owner, dependent issue,
   compatibility period, fields, authority effect, and proof.
3. Prove historical references remain readable, while hidden readers and
   legacy ready status cannot authorize current work.
4. After this audit lands, #1701 may migrate durable campaign records and
   explicit campaign views. It must rerun the audit against its exact head.

## Migration proof

- Before each #1701 slice, run `cargo xtask active-goal-authority-audit` and
  inspect both generated projections.
- Treat `migration_ready = false`, an unclassified discovery, or a
  contradiction as a stop condition.
- Verify each changed consumer with the positive and negative proof named by
  its inventory row.
- Preserve live GitHub and local ownership as unavailable unless supplied by a
  separate captured-input portfolio contract.

## Rollback

Revert the authority-migration slice while retaining this audit, durable
campaign records, and historical evidence. A compatibility reader may be
restored only as explicitly non-authoritative. Rollback must not restore global
selection or mutation authority to legacy status, branch, lane, or work-item
fields.

## Non-goals

- No authority migration in this prerequisite.
- No live portfolio, issue priority, claim, worktree, writer, merge, or release
  action.
- No replacement for cargo-allow structural authority.
