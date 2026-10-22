# ADR 0020: Repair Artifacts Carry Producer Identity

Status: accepted

Date: 2026-07-22

Artifact ID: RIPR-ADR-0020

## Context

`ripr agent verify` compares saved static snapshots before producing movement
evidence. A path and plausible JSON shape do not establish that either input
was produced by RIPR, belongs to the selected repository, or is unchanged
since production. Treating those fields as sufficient lets fabricated input
reach movement and receipt consumers.

## Decision

Repo-exposure snapshots consumed by `agent verify` carry a producer-owned
`artifact` envelope containing:

- artifact kind and identity schema;
- producer tool/version and commitment canonicalization;
- analyzed root and Git HEAD when available;
- analysis format, mode, base revision, and worktree state;
- a SHA-256 commitment over the exact JSON bytes with the commitment value
  replaced by the fixed `raw_json_placeholder_v1` value.

The producer computes the commitment in a bounded-memory two-pass writer.
`agent verify` parses and validates this envelope, checks root and revision
compatibility, verifies the commitment, and reports pair currentness. Invalid
or unavailable identities fail before static movement is calculated.

Amendment (#3027): the pair token originally collapsed every mixed pair into
`dirty_worktree`. The closed pair vocabulary is now `current`,
`historical_noncurrent`, `historical_before_current_after`,
`current_before_historical_after`, `dirty_before`, `dirty_after`, and
`dirty_both`, computed by one shared mapping in
`crates/ripr/src/agent/artifact.rs`; `dirty_worktree` stays reserved for
per-artifact evidence.

Amendment (#9103): diff-scoped check findings are a second verify subject.
`ripr check --format check-findings-json` renders the unbounded `check
--format json` document with the same `artifact` envelope (`kind:
"check_findings"`, `analysis.format: "check-findings-json"`, matching
`analysis.command`). `agent verify` and `agent receipt` select the validator
from the declared kind, and both kinds run the one identity, commitment and
Git-provenance validator, so the findings kind is not a weaker twin: a
hand-edited, unenveloped, foreign-root, incomparable, or reversed snapshot is
rejected exactly as a repo-exposure snapshot is. The findings kind adds two
fail-closed requirements, because a gap's absence is what reads as "closed":
`analysis_outcome.analysis_complete` must be `true`, and the findings array
must not be budget-bounded (`document_complete` must not be `false`; the
producer never bounds this format), and the run must not be a partitioned
diff (`analysis_scope`) or carry run limitations. The identity pins mode and
base, not the diff, so absence of a gap is never closure evidence by itself;
gap-bound consumers must require the gap present and `exposed`. Gaps are keyed by canonical gap id (the
producer-stated `canonical_gap_id`, or the TypeScript identity derived from
the content-addressed finding id), which is how movement is compared. The
analysis format is part of the input identity, so a findings artifact and a
repo-exposure artifact are never comparable to each other.

This is an integrity/currentness boundary, not a signature system and not
runtime mutation proof. Signature, command execution, and receipt issuance
remain separate follow-up contracts under #1941.

## Consequences

Positive:

- Legacy or hand-authored snapshots cannot reach movement merely because they
  contain `seams` or `findings` fields.
- Large repo-exposure artifacts retain streaming output and bounded memory.
- Dirty and historical evidence stays inspectable with an explicit disclosure
  rather than being presented as current.

Costs and limits:

- Existing static calibration fixtures must be wrapped or generated through
  RIPR before `agent verify` can consume them.
- A producer marker and digest do not replace signing; a compromised producer
  can still emit a self-consistent artifact.
- `receipt` provenance and verification-command execution are intentionally not
  claimed by this decision.

## Alternatives Considered

- **Trust path and JSON shape.** Rejected: a plausible snapshot would then
  reach movement and receipt consumers without evidence that RIPR produced it
  (see Context).
- **A signature system.** Not chosen here: the commitment is an
  integrity/currentness boundary, and signing is left to the follow-up
  contracts under #1941 (see Decision).
