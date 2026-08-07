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

## Publication law

Artifact copies are staged in a sibling temporary directory inside the attempt
directory and renamed into `artifacts/` once complete. The manifest is written
to a temporary file, synchronized, and linked into place. A manifest is not
visible as complete until all attempt-specific artifact copies have been
published, and a mid-staging failure leaves no partial `artifacts/` set.

The before phase holds a per-repository lock
(`target/ripr/repair-attempts/.before.lock`) across workflow execution and
attempt publication, so a concurrent before phase cannot publish another
invocation's workflow artifacts under this attempt's identity.

Source artifacts must canonicalize inside the selected repository root.
Duplicate roles or destination file names fail closed. Attempt destinations
are immutable: an existing attempt directory, artifact, or manifest is never
reused or overwritten, and a failed begin removes the reserved attempt
directory.

## Current state

Schema `0.1` emits exactly:

```text
awaiting_edit
```

Later slices will add the operational and movement transitions required by
#2927. They must not reinterpret `awaiting_edit` as verified, improved, closed,
or safe to merge.

## Non-claims

A prepared attempt does not mean:

- RIPR authored or applied a test edit;
- the selected repair is correct;
- verification ran;
- the static gap improved or closed;
- mutation testing ran;
- the repository is safe to merge;
- the attempt is yet resumable through `--attempt`.

The JSON contract is `schemas/ripr/repair-attempt.schema.json`.