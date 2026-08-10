# RIPR-SPEC-0149: History-preserving back-sync verifier

Status: accepted

Owner: release control / swarm operations

Created: 2026-08-09

Linked issue: #3100

## Contract

`cargo xtask back-sync verify` consumes exact `SWARM_BEFORE`,
`SOURCE_RELEASE_HEAD`, reviewed `BACK_SYNC_TREE`, and an exact candidate `K`.
It fails closed unless `K` is a two-parent commit with ordered parents
`[SWARM_BEFORE, SOURCE_RELEASE_HEAD]`, both parents are reachable from `K`,
and `K^{tree}` equals the reviewed tree. The source and swarm declared main
heads must identify the exact release pair. The swarm head must be exactly
`SWARM_BEFORE` before transport or exactly `K` after transport; this is the
expected-head guard.

The receipt requires and validates release/changelog/publication evidence
bound to the requested version, released source head, K, and reviewed tree.
It requires parseable policy-before, temporary-approved-exception, and
restoration evidence, and fails closed when merge commits are not disabled
before and after. It requires every retained swarm development surface and
rejects source publication workflow/settings paths changed into K. The
verifier reads these inputs and never mutates refs, branch protection,
repository settings, tags, releases, publication channels, or metadata.

## Non-goals

- merge construction, cherry-pick, squash, rebase, force-push, or transport;
- source-side exact-`J` verification;
- policy mutation, publication, signing, or release action;
- a claim of release correctness or artifact adequacy.

## Proof mapping

- `xtask/src/reports/back_sync.rs` contains exact-input validation, receipt
  rendering, and synthetic single-parent/ordered-join graph tests.
- `docs/BACK_SYNC_VERIFIER.md` contains the repeatable operator command.
- `docs/RELEASE.md` and `docs/swarm-development.md` define the J/K boundary.
