# Consumed Rust bytes at refresh preparation

Owner: [#1765](https://github.com/EffortlessMetrics/ripr-swarm/issues/1765).
This is a prerequisite for #1602/#4807/#3089, not their completed snapshot or
all-language input-identity contract. The design and observed regression are
recorded in [the implementation packet](https://github.com/EffortlessMetrics/ripr-swarm/issues/1765#issuecomment-5892466081).

## Observed defect

The actual saved-worktree producer completed two findings from A. With its
refresh generation unchanged, disk and buffer became B before preparation.
Preparation reread B, attributed the A findings to B, and cleared quarantine.
The focused test at `4c0962ead5d9280c1a0b1848c7e0500d6de6d580` compiled and
failed on that explicit attribution condition: one failed, zero passed or
ignored. Later signature and refusal checks did not execute on that RED.
Earlier setup, diagnostic compilation and Git admission failures are separate
retained evidence; they do not establish the product defect.

## Private connection

Record workspace-relative paths and SHA-256 of the raw buffers loaded by the
Rust diff/repo adapters before cache lookup. Identical observations are
idempotent; missing or conflicting observations cannot supply a digest.
Cached normalized source and diagnostic geometry cannot mint commitments.
Carry the observations through internal results, a private app wrapper and the
completed workspace snapshot. Preparation uses them for `.rs` documents;
absent, out-of-root or conflicting commitments quarantine instead of borrowing
disk or didSave state. Existing commit and cancellation ownership remain.

Public `CheckOutput`, source-range DTOs and existing origin-wrapper signatures
remain unchanged. `AnalysisResult` already has private fields. Other extensions
retain inherited fallback, including non-`.rs` Rust-language buffers. Reads
outside the captured vectors do not establish complete include/dependency
identity. #1602/#4807 retain that broader work. Preserve #4830 identity-law and
#4844 RepairAttempt ownership.

## Proof

The real Backend A-to-B regression must retain A attribution and quarantine B,
then a fresh real B analysis must carry B, clear quarantine and serve the
committed diagnostics. Actual saved producers cover cold misses/stores and warm
hits with plain, BOM, CRLF and invalid-UTF-8 raw inputs. Test-only telemetry is
populated from actual cache stats. Unit controls cover unavailable/conflicting
paths, quarantine recovery and unchanged other-extension behavior.

Run the focused library namespaces `lsp::tests::consumed_source_tests::` and
`analysis::consumed_source::tests::` against the admitted freshly compiled
library executable. Candidate compilation/runtime and owning guards are not
established by source review or the historical RED. Exact receipts accompany
the candidate before publication.

## Boundary and rollback

No public schema, cache generation, portable identity, snapshot mint, scheduler
engine, geometry, discovery, ranking, MCP analysis service or release change.
Revert the private connection normally if necessary; retain the discriminating
tests and proof. Parent issues remain open.
