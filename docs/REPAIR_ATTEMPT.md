# Repair attempts

A repair attempt is the durable identity for one bounded RIPR repair transaction.
It is separate from an analysis-refresh attempt, LSP scheduler generation, gap
identity, or receipt identity.

This document describes schema `0.1`, the first implementation slice under
#2927.

## Current sequence

```text
current producer-owned seam
-> ripr agent repair --phase before
-> existing workflow, brief, packet, and before snapshot complete
-> attempt-specific copies and digests are retained
-> attempt.json is published atomically in awaiting_edit state
-> human or external agent edits one focused test
-> existing seam-selected --phase after compatibility route
-> repository delta and edit-cage verdict are persisted in attempt.json
-> receipt admission revalidates the selected seam, packet, baseline, HEAD,
   delta, and compliant verdict against the same attempt
```

The next implementation slice will make the after phase consume
`--attempt <repair-attempt-id>` and bind after/verify/receipt evidence to this
manifest. Schema `0.1` does not claim that finishing by attempt ID already
exists.

## Location

Each successful before phase creates:

```text
target/ripr/repair-attempts/<repair-attempt-id>/
├── attempt.json
└── artifacts/
    ├── workflow.json
    ├── commands.md
    ├── agent-brief.json
    ├── before.repo-exposure.json
    └── agent-packet.json
```

The pre-existing compatibility artifacts under `target/ripr/workflow/` remain
available. The attempt directory receives copies so a later before phase cannot
silently replace the evidence attached to an earlier attempt.

## Identity

`repair_attempt_id` has the closed form:

```text
repair-attempt-<24 lowercase hexadecimal characters>
```

It is an operational identity derived from repository root, concrete Git HEAD,
seam ID, creation time, process ID, and a process-local nonce. It is not a
portable semantic digest and is not used to identify the underlying gap.

The manifest separately retains:

- canonical repository root;
- concrete 40-character repository HEAD;
- RIPR producer version;
- selected seam ID;
- creation time as telemetry;
- role, repo-relative path, byte count, and SHA-256 digest for every retained
  before-phase artifact;
- the currently supported exact next command;
- limitations and non-claims.

After a successful after phase, `after` records the durable `repair_attempt_id`,
current repository HEAD,
packet and delta SHA-256 commitments, currentness, and the edit-cage verdict.
Terminal states are `ready_to_finish`, `failed`, `stale`, or `incomparable`;
only `ready_to_finish` with a compliant verdict can authorize a receipt.

## Publication law

Artifact copies are staged in a sibling temporary directory inside the attempt
directory and renamed into `artifacts/` once complete. The manifest is written
to a temporary file, synchronized, and linked into place. A manifest is not
visible as complete until all attempt-specific artifact copies have been
published, and a mid-staging failure leaves no partial `artifacts/` set.

The before phase holds a per-repository lock
(`target/ripr/repair-attempts/.before.lock`) across workflow execution and
attempt publication, so a concurrent before phase cannot publish another
invocation's workflow artifacts under this attempt's identity. Acquisition is
non-blocking: a concurrent before phase fails closed with a bounded error
instead of waiting. The lock is an OS file-handle lock released on drop or
process exit, so it cannot go stale.

Source artifacts must canonicalize inside the selected repository root.
Duplicate roles or destination file names fail closed. Attempt destinations
are immutable: an existing attempt directory, artifact, or manifest is never
reused or overwritten, and a failed begin removes the reserved attempt
directory.

## Current state

Schema `0.1` emits `awaiting_edit` before the edit and a terminal after-phase
state after the edit-cage comparison. A receipt revalidates every retained
artifact's path, byte count, and digest, requires the manifest root and
baseline root to equal the selected repository, and rejects stale, tampered,
replayed, wrong-seam, and incomparable attempts. Receipt binding uses the
durable attempt identity when the repair command creates the receipt, while
the standalone compatibility route remains fail-closed when multiple terminal
attempts exist for one seam. The compatibility route is still seam-selected
when invoked directly; the repair after phase binds the receipt to the durable
attempt that it just finished.

## Non-claims

A prepared attempt does not mean:

- RIPR authored or applied a test edit;
- the selected repair is correct;
- verification ran;
- the static gap improved or closed;
- mutation testing ran;
- the repository is safe to merge;
- the attempt is yet resumable through `--attempt`;
- a compliant edit-cage verdict proves test correctness or mutation behavior.

The JSON contract is `schemas/ripr/repair-attempt.schema.json`.
