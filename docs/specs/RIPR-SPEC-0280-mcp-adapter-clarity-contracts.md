# RIPR-SPEC-0280: MCP adapter clarity contracts — encoded resource ids, honest paging echoes, and status recovery parity

Status: proposed

Owner: mcp

Created: 2026-10-19

Linked issues:

- #8736 (percent-decode the captured resource-id segment before lookup;
  document the id grammar)
- #8735 (the typed page must echo the effective inventory limit, and the
  declared input cap must be enforced at the dispatch edge)
- #8737 (the status resource carries the same restart recovery as the
  status tool on an unavailable root)
- #8734 (the two same-shaped per-item tool descriptions disambiguate each
  other)
- #8859 (review follow-up: raw/decoded resource-id collisions fail closed
  with `id_ambiguous` instead of silently picking an item)

Support-tier impact:

- None. The MCP adapter runs the same read-only static analysis the CLI and
  LSP run in-process; this spec changes only wire-facing documents and
  descriptions, not language support claims.
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- None. No new output format, gate, badge, or report file is added; the MCP
  documents stay wire payloads bounded by the existing MCP byte caps, and
  the new `id_ambiguous` failure code travels inside the existing typed
  failure envelope.

## Problem

Three adapter surfaces claimed more than they enforced, and one recovery
route looped forever:

1. Resource-template ids were captured by raw `strip_prefix` with no
   percent-decoding, but gap and repair-card canonical ids embed raw
   workspace-relative file paths (spaces, `#`, and non-ASCII are literal, in
   Python since the producer leaves `%20` intact). A URI-emitting client's
   encoded id never matched, failed with `item_not_found`, and the recovery
   ("list the ids, then retry") failed identically forever.
2. `ripr_list_repair_attempts` silently clamped a caller-supplied `limit` to
   the 256-row cap but the typed `page` block echoed the unclamped request,
   so a `limit: 1000` call reported `limit: 1000` with 256 rows and the
   output schema asserted no maximum.
3. On an unavailable root the `ripr_workspace_status` tool appended the
   restart recovery sentence, but the `ripr://workspace/status` resource —
   presented as the same status projection — returned only the bare document
   with an opaque `error_code`.
4. `ripr_get_gap` and `ripr_get_repair_card` expose the same input shape and
   both claimed per-item completeness, so a routing model could not tell the
   analysis-evidence read from the repair-intent card.

A naive decode-first or raw-first precedence then turned case 1 into a
wrong-item hazard: when both `a b.py` and `a%20b.py` carry gaps, the segment
`a%20b.py` resolves as the literal-percent file raw and as the space file
decoded, and either fixed order silently returns the item the client did not
name.

## Behavior

`ripr mcp --stdio` keeps the bounded read-only authority of ADR 0022 and
changes four observable contracts:

1. **Resource-id decoding with explicit collision failure.** The
   `ripr://gap/{canonical_id}` and `ripr://repair-card/{canonical_id}`
   routes resolve one captured segment through an exact-id lookup:
   - the raw capture is tried first, so legacy raw URIs (including ids
     whose literal `%` spelling arrives unencoded) keep working;
   - when the raw capture misses with `item_not_found`, the segment is
     percent-decoded once (each `%XX` with two hex digits decodes to its
     byte; a stray `%` or a short tail stays literal; a decode that changes
     nothing or is not UTF-8 is refused) and the decoded form is tried
     instead;
   - when the raw capture succeeds **and** the segment also decodes to a
     different existing id, both spellings name two distinct items, so the
     read fails closed with the typed `id_ambiguous` failure rather than
     silently picking one; its recovery names the equivalent tool route,
     which takes the `canonical_id` as a plain JSON string with no URI
     encoding.
   Both resource template descriptions declare this id grammar and name the
   tool route as the fallback. Snapshot, repair-attempt, and receipt ids
   stay on their existing path-free grammars and do not decode.
2. **Honest inventory paging.** `ripr_list_repair_attempts` declares
   `maximum: 256` on the input `limit`; a request above the cap is rejected
   as invalid params (`-32602`) at the dispatch edge instead of being
   silently clamped; and `page.limit` plus the bound continuation echo the
   effective limit actually served, so `returned` always reconciles against
   `page.limit`.
3. **Status-resource recovery parity.** When the workspace root is
   unavailable, the `ripr://workspace/status` resource payload carries the
   same recovery sentence as the `ripr_workspace_status` tool result as a
   second text content entry (it rides inside `contents` because the read
   path round-trips the SDK's typed resource result, which drops unknown
   top-level fields), naming the cause and the
   `ripr mcp --stdio --root <repository>` restart route. On a ready root the
   payload is unchanged.
4. **Per-item tool disambiguation.** The `ripr_get_gap` description names
   `ripr_get_repair_card` as the read for repair intent (fix instruction,
   edit cage, done-when goals, typed next action) and narrows its own claim
   to "complete bounded analysis evidence"; the
   `ripr_get_repair_card` description names `ripr_get_gap` as the lighter
   analysis-evidence read.

## Required Evidence

- A two-file integration fixture (`a b.py` and `a%20b.py`, both with
  matching Python gaps) proving: the raw percent spelling fails closed with
  `id_ambiguous` on the gap route and never silently returns the other
  file's document on the repair-card route; the fully encoded `%2520`
  spelling resolves to the literal-percent file's document; the raw space
  spelling resolves to the space file's document; and the tool route serves
  both ids as plain JSON strings. The shared resolution helper is unit-pinned
  for both routes, so the repair-card route fails with `id_ambiguous`
  whenever both lookups resolve.
- Unit tests pinning the resolution law (raw hit, raw hit with decoded
  miss, raw miss with decoded hit, both-exist collision, no-op decode
  refusal, non-UTF-8 refusal) and the decoder's escape grammar.
- An inventory paging test pinning the declared input maximum, the
  `-32602` over-cap refusal, and the effective-limit echo in `page.limit`
  and the bound continuation.
- An unavailable-root test pinning the second recovery content entry on
  both root-error causes.
- The description contract test pinning the cross-reference wording in both
  tool descriptions.

## Non-Goals

- No edit, execute, or mutation authority anywhere in the adapter; the
  repair boundary stays `none_declared` until `ripr_prepare_repair` binds a
  transaction.
- No change to canonical-id production: Python gap ids keep embedding the
  raw file path, and Rust ids keep their stable-path `%25` escaping. The
  adapter resolves the ambiguity it is handed; it does not redefine id
  grammars.
- No decoding on the snapshot, repair-attempt, or receipt routes (their id
  grammars are path-free and URI-safe already).
- No new spec numbers, output formats, gates, badges, or report files.

## Acceptance Examples

- `ripr://gap/gap:python:src/a%2520b.py:accepts:...` returns the
  literal-percent file's gap document (decoded once, unambiguous).
- `ripr://gap/gap:python:src/a b.py:accepts:...` returns the space file's
  gap document (raw capture, no decodable escape).
- With both files present, `ripr://gap/gap:python:src/a%20b.py:accepts:...`
  fails with `id_ambiguous` naming both spellings and the JSON-string tool
  route as recovery; the matching `ripr://repair-card/...` read fails
  closed through the same resolution helper (it answers `id_ambiguous`
  whenever both lookups resolve, and an earlier typed refusal such as
  `seam_not_found` otherwise — never the other file's card).
- `ripr_list_repair_attempts` with `{"limit": 1000}` answers invalid params
  `-32602`; with `{"limit": 2}` the typed page echoes `"limit": 2`.
- On a root with `repository_marker_missing`, `ripr://workspace/status`
  carries a second text content entry containing
  "Workspace unavailable (repository_marker_missing)" and the
  `ripr mcp --stdio --root <repository>` restart route.

## Test Mapping

- `crates/ripr/tests/mcp_stdio.rs::encoded_resource_ids_fail_closed_when_raw_and_decoded_both_exist`
- `crates/ripr/src/mcp/server_tests.rs::resource_id_document_resolves_raw_decoded_and_ambiguous_spellings`
- `crates/ripr/src/mcp/gaps.rs::tests::percent_decoding_resource_id_segments`
- `crates/ripr/tests/agentic_bench_mcp.rs::attempt_inventory::inventory_page_limit_echoes_the_effective_cap`
- `crates/ripr/src/mcp/protocol.rs::tests::tool_descriptions_state_positive_contracts_and_bounds`

## Implementation Mapping

- `crates/ripr/src/mcp/server.rs::resource_id_document` — shared
  raw-first/decoded-retry/collision-fail-closed resolution for the gap and
  repair-card resource routes.
- `crates/ripr/src/mcp/gaps.rs::percent_decode_resource_id` — the escape
  grammar (strict `%XX`, literal stray `%`, no-op and non-UTF-8 refusal).
- `crates/ripr/src/mcp/workspace.rs::CODE_ID_AMBIGUOUS` — the typed
  collision failure code.
- `crates/ripr/src/mcp/attempt_inventory.rs` — the declared input maximum,
  the effective-limit echo, and the continuation echo.
- `crates/ripr/src/mcp/server.rs::optional_capped_page_limit` — the
  dispatch-edge over-cap refusal for the inventory tool.
- `crates/ripr/src/mcp/protocol.rs::status_resource_result` — the second
  recovery content entry on an unavailable root.
- `crates/ripr/src/mcp/protocol.rs` — the tool and resource-template
  description contracts.

## Metrics

- `mcp_resource_id_ambiguity_fail_closed`: collision reads (raw and decoded
  spellings both resolve to distinct existing ids) refused with
  `id_ambiguous` instead of silently returning one item. Target: every
  collision fails closed; the two-file integration fixture and the
  unit-pinned resolution law are the discriminator. No metric claims more
  than it enforces; the paging echo, status recovery parity, and
  description cross-references stay covered by their pinned tests above.
