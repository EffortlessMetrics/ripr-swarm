# Readiness snapshot rehearsal receipts (#4640)

Spec: RIPR-SPEC-4640

Manifest-only JSON fixtures for `cargo xtask readiness-snapshot` (#4640). They
model versioned #1703-style no-publish rehearsal receipts consumed by the
snapshot command; they do not run package builds or network actions.

## Given

- a workspace root whose lockfile and toolchain digests match each receipt
  `identity` block (fixture JSON may pin an illustrative `source_sha`; xtask
  tests rebind receipts to `HEAD` so shallow CI checkouts need not fetch old
  commits);
- optional supply-chain component rows on the supply-chain fixture.

## When

- `build_snapshot` loads a fixture path via `--rehearsal-receipt` or tests call
  `build_snapshot` directly;
- identity inputs are collected from the live workspace for compatibility checks.

## Then

- a matching complete receipt marks `toolchain_reproducibility` and
  `package_and_installability` as `pass` while other dimensions stay
  `not_proven`;
- a wrong `source_sha` marks affected dimensions `stale`;
- supply-chain fixtures keep `dependency_policy`, `sbom`, and `provenance_plan`
  as distinct evidence producers.

## Must Not

- treat partial or stale rehearsal as a ship verdict;
- rebuild packages inside the snapshot command;
- collapse supply-chain producers into one synthetic pass row.
