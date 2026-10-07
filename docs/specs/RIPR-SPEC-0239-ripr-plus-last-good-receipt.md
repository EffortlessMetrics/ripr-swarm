# RIPR-SPEC-0239: `ripr plus` last-good receipt

Status: proposed

Owner: app

Created: 2026-10-05

Linked issues:

- #5310 (a mistyped `ripr plus` path cost the previous receipt)
- #5595 (help and messages named copies that were not kept)
- #6295 (a receipt `ripr plus` composed was never kept)
- #6698 (a kept Markdown could belong to a different run than its JSON)
- #6714 (tests checked only the message, not the files)

Support-tier impact:

- None. No language or output support claim changes.
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- None. No gate, badge, schema or report-path change; the last-good paths
  already exist.

## Problem

`ripr plus` writes `target/ripr/reports/ripr-plus.{json,md}`. A run whose
named artifact cannot be read or composed still writes an `indeterminate`
error receipt there, so CI keeps a record. Without a copy, that failure would
also destroy the previous composition. Every receipt `ripr plus` composes is
`indeterminate` with cause `quality_evidence_incomplete`, so a rule that
skipped every `indeterminate` receipt kept nothing `ripr plus` itself wrote.

## Behavior

- "Last good" means the last receipt a run actually composed.
- Before a read or compose failure overwrites `ripr-plus.json`, the receipt
  there is copied to `ripr-plus.last-good.json` when its `status` is not
  `indeterminate`, or is `indeterminate` with `machine_readable_cause`
  `quality_evidence_incomplete`.
- An error receipt (`indeterminate` with any other cause, such as
  `evaluation_error` or `evaluation_timeout`), a receipt with no `status`, and
  unparseable JSON are never copied, so a second failure cannot replace the
  kept copy with an error.
- The last-good JSON is saved first. Last-good Markdown is kept only when the
  canonical Markdown is the projection of that JSON and this call saved it;
  the copy is skipped when the saved path is already a regular file with those
  bytes. A leftover
  Markdown from another run is dropped only after that JSON save, so a failed
  JSON copy leaves a previous matching pair intact and a newer JSON never sits
  beside another run's Markdown. When the canonical Markdown cannot be read,
  the JSON is still saved, leftover last-good Markdown is dropped, the message
  does not name last-good Markdown as kept, and the read failure is a
  Markdown-only failure.
- The failure message names only the copies actually written. A composed copy
  is labelled with its status and cause. Every message says the copy describes
  an earlier run, may be stale for the current HEAD, and is not current
  evidence; when nothing was copied, it says any copy still on disk is from an
  earlier run.
- A `--check` failure on a composed receipt writes that receipt and makes no
  last-good copy. No run removes a copy: one an earlier failure left stays
  until a later failure replaces it, and every message says such a copy is
  from an earlier run.
- The canonical path always carries the failed run's error receipt, so no
  reader mistakes a kept copy for the current result.

## Required Evidence

- A composed receipt is kept byte for byte, JSON and Markdown, when a later
  run fails to read its artifact.
- Error receipts of both causes are never kept.
- A legacy `pass` receipt is kept and is not replaced by a second failure.
- Blocked destinations are not written, and the message names only the copies
  that were.
- `--check` on a valid input makes no last-good files, and leaves a copy an
  earlier failure made byte for byte.
- A mismatched canonical Markdown is not named as kept, and the leftover
  last-good Markdown is dropped after the JSON save, including a read-only
  leftover file. A failed JSON copy leaves a previous matching pair intact
  and names no Markdown as kept.
- When the canonical Markdown cannot be read, JSON is still saved, leftover
  last-good Markdown is dropped, and the message does not name last-good
  Markdown as kept.

## Non-Goals

- Treating a kept copy as evidence for any gate or badge.
- A dedicated run-id field on the last-good pair. Pairing is by exact
  Markdown projection of the saved JSON; two runs that render identical
  Markdown cannot be told apart.

## Acceptance Examples

1. `ripr plus --repo-exposure-summary summary.json` exits 0; then
   `ripr plus --gap-ledger missing.json` exits 2, `ripr-plus.json` carries
   cause `evaluation_error`, and `ripr-plus.last-good.{json,md}` equal the
   first run's receipt. The message says "The previous receipt (status
   `indeterminate`, cause `quality_evidence_incomplete`) is kept at ...".
2. With a `pass` receipt on disk, `ripr plus --repo-exposure-summary
   summary.json --check` exits 2 and writes no last-good file.

## Test Mapping

- `crates/ripr/src/app/ripr_plus.rs::tests` — keep rule, message branches and
  blocked destinations.
- `crates/ripr/tests/cli_smoke.rs::plus_failed_run_keeps_the_composed_receipt_and_check_keeps_none`
  — end to end through the binary.
- `crates/ripr/tests/cli_smoke.rs::plus_help_exits_cleanly` — help states the
  rule.

## Implementation Mapping

| Surface | Responsibility |
| --- | --- |
| `crates/ripr/src/app/ripr_plus.rs` | `keep_last_good_receipt`, `compose_and_write_receipt`, help text |

## Metrics

- None.
