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

## Plan-only extension (#1781)

`rehearsal-receipt.plan.json` is illustrative schema 1.1 planning data. It
shares the existing receipt kind and Rust model with 1.0, adding a required
`plan` block. The block binds the source tree, host OS/architecture, plan-only
profile, read/tool inventory, empty write/cache inventories and explicit
no-mutation observations. All present components are `not_run` without artifact
digests; empty server/install inventories establish no coverage.

The schema is `schemas/ripr/readiness-rehearsal-plan.schema.json`. Schema 1.0
fixtures remain valid and serialize without `plan`; 1.0 with plan metadata and
1.1 without it are refused. Old 1.0 readers reject 1.1 instead of accepting new
semantics silently. Other profiles/versions require a separately reviewed
contract extension. #4850 retains executed component fan-in.

`cargo xtask readiness-rehearsal --sha HEAD --plan --json` emits to stdout only
and requires clean current HEAD (including no untracked files). It performs
read-only Git and identity-file inspection; it creates no report, cache, package
or installation. `--plan` and explicit `--sha` are required. Execution options
are unavailable. Callers may retain stdout outside the source tree; redirecting
inside tracked/untracked source before invocation makes the cleanliness check
refuse. The two cleanliness observations do not lock out concurrent writers;
run in an exclusively owned checkout. No generated timestamp enters the receipt.

Identity inputs come from immutable commit blobs even for current HEAD. All Git
probes disable replacement objects, fsmonitor, lazy fetching, transports, prompts
and paging; missing objects fail closed. Inherited Git trace and stdio redirection
controls and Git test instrumentation are removed before the first probe. Explicit zero trace2 targets also
prevent configuration-based trace output. Before status, source inspection rejects
configured clean/process filters, index flags that can mask changes, and gitlinks
whose recursive status might invoke an unchecked nested helper. This deliberately
refuses filter-enabled workspaces (including global Git LFS configuration), sparse
or assume-unchanged indexes and submodules. The shared snapshot cleanliness probe
has the same conservative limitation; receipt schema 1.0 parsing remains supported.
The index inventory runs explicitly at the repository root without pathspecs,
so member-directory calls and inherited literal/glob pathspec modes cannot hide
masking flags or gitlinks outside the current subdirectory. Root path parsing
preserves whitespace and refuses non-UTF-8 paths.

Git status is insufficient to establish clean bytes: a same-size edit with
preserved mtime can evade its stat cache. The producer separately enumerates the
captured commit's full tree and compares raw tracked-file blob hashes using
`git hash-object --no-filters` without object writes. Hash batches are bounded to
64 files and 8 KiB of arguments, with exact result-count checks. Tracked paths
must be relative UTF-8 paths with regular files and directory parents, without
symlinks or gitlinks. Unix owner execute bits must match the committed mode.
Raw-byte verification can refuse CRLF, working-tree encoding and ident/filter
conversions that Git status would otherwise consider clean. It does not lock
the checkout against concurrent writers. Shared split indexes are refused before
index reads because Git can freshen their timestamps despite optional-lock
suppression; index revision expressions are unsupported. Preserved-mtime, trace, batch-coverage,
mode and parent-symlink regressions cover these refusals.

Consuming schema 1.1 leaves rehearsal-derived toolchain, package/install and
supply-chain dimensions `not_proven`. Existing independent evidence adapters
retain their own authorities. Plan-only receipt data cannot promote readiness,
even when a caller bypasses parsing. Tests challenge missing/unknown metadata,
mutation claims, unsupported profiles, false component passes, deterministic
planning and CLI refusal. Source tests and this fixture never establish artifact
or platform qualification.
