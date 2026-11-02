# Verification Run Snapshot Binding Corpus

Issue: #6347 (VP25A product binding model)

## Given

A pure, process-free RIPR binding must compare an exact verification-plan
requested subject against an accepted shared `RepositorySnapshotEnvelopeV2`
reference and prepared acquisition facts without acquiring or executing anything.

This directory commits ten hostile JSON cases in `corpus.json` that exercise
committed, staged, saved-worktree, overlay, captured-external, partial,
relocation, reordering, and plan-identity invalidation boundaries.

## When

`cargo test -p ripr --lib analysis::verification_plan::snapshot_binding` loads
the corpus from this directory, recomputes compatibility and portable binding
identity, and fails closed on drift.

## Claim boundary

Fixture inputs are synthetic identities only. They do not prove snapshot
acquisition, cargo-proof materialization, or provider execution behavior.
