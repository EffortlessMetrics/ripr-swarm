# RIPR-SPEC-0454: Operator Queue Conflict Notices

Status: proposed

Owner: CI operations

Linked issue: [#14705](https://github.com/EffortlessMetrics/ripr-swarm/issues/14705).

## Problem

A maintainer needs to tell a queued PR's author which paths conflicted and which
commits were compared. If GitHub processes a comment but the response or local
receipt fails, another submission can duplicate the notice.

## Behavior

A preview binds the PR, queue entry, observed main and head to a fresh Git
comparison. A local operator can explicitly request a comment when every
conflicted path has an attributed transition. Before submitting, the command
checks for a trusted existing marker and rechecks membership and viewer.
The comment describes the observed commits even if membership subsequently
changes. Operators serialize attempts for a PR.

## Required Evidence

Each attempt commits its pending intent before submission and separately records
the terminal outcome. A failed pre-dispatch check records `not_submitted`.
After dispatch, an unavailable acknowledgement records `unconfirmed`. A valid
created-comment response carries a positive ID. An incomplete terminal write
preserves the intent and reports both the observed remote outcome and the local
failure. A read of comments on the original PR can reconcile the marker after
the queue entry expires.

## Non-Goals

The command does not schedule conflict scans, resolve conflicts, authorize a
merge, or guarantee an atomic GitHub membership check and comment write.

## Acceptance Examples

- A replaced queue entry prevents submission.
- A failed intent commit prevents submission.
- A lost acknowledgement and failed terminal commit retain the pending bytes
  and direct the operator to inspect GitHub.
- A successful submission followed by a storage failure reports the comment ID.
- An HTTP failure reports the numeric status and omits response headers and body.

## Test Mapping

The [notice tests](../../xtask/src/reports/merge_queue/conflicts/notice/tests.rs)
exercise queue identity, attributed Git conflicts, trusted marker recognition,
delivery and storage faults. The traceability fragment lists the test symbols.

## Implementation Mapping

The [notice command](../../xtask/src/reports/merge_queue/conflicts/notice.rs)
owns GitHub calls and receipts. Its [Git comparator](../../xtask/src/reports/merge_queue/conflicts.rs)
owns path-specific transitions. The [operator guide](../CI.md#preview-or-post-one-queued-conflict-notice)
documents reconciliation.

## Metrics

Tests count submissions made by the fake transport and compare pending receipt
bytes before and after a terminal storage failure.
