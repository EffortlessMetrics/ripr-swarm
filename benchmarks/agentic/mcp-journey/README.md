# MCP journey benchmarks: B2 happy path, B3 negative authority

Executable oracle: `crates/ripr/tests/agentic_bench_mcp.rs`.
Machine contract: `spec.json`. Static pre-refresh probe script: `b3a.probes.ndjson`.

## Fixture

`input/base` is a minimal Cargo package committed on branch `main`;
`input/journey/src/lib.rs` closes the predicate (`>` to `>=`) on branch
`journey`. The harness copies `input/` into a temp dir, commits both
branches with git, and points `ripr mcp --stdio --root <fixture>` at it.
Analysis diffs the resolved default base (`main`) against `journey`, so the
refresh commits a snapshot with the boundary gap. Data stays small: four
source files, no generated output.

## B2: happy-path journey

One stdio session drives the full tool sequence: status, refresh,
list_gaps, get_gap, prepare_repair (twice), get_repair_attempt,
get_receipt_status. The oracle requires: refresh `completed` with a
non-empty snapshot id; status re-binds that id; list_gaps counts
consistent (`items.len == selected`, `omitted_items.len == omitted`,
`selected + omitted == total`, `selected <= eligible <= total`,
`selected_bytes <= complete_bytes`); at least one gap; get_gap binds the
snapshot, item, and changed behavior; the two prepares are identical; and
the repair chain binds attempt, receipt, and the gap link when the
producer reports ready (or typed `attempt_not_found` for unknown ids when
not ready).

## B3: negative authority

Fresh sessions per probe group: pre-refresh (`no_snapshot` for
list/get/prepare, `attempt_not_found` for attempt/receipt reads),
stale-snapshot ids (`stale_snapshot` plus the current id), unknown item
and attempt ids (`item_not_found`, `attempt_not_found`), and a pipelined
refresh-plus-followers batch that must stay sequential and coherent
(refresh completes, every follower binds that snapshot). Every failure
must be a typed envelope (`code`, `detail`, `recovery`), never a silent
null; every stdout frame must fit the advertised 131072-byte response
bound; stderr stays empty; and the fixture tree gains nothing outside
refresh cache (no repair-attempt or receipt artifacts).

## Anti-gaming twins

Replay twin: preparing a different item (or the same item under a stale
snapshot) must NOT equal the first prepare document, so a constant
responder fails. Oversize twin: a ~200 KB argument (under the input cap)
must fail closed with a typed code and a bounded reply while the session
keeps serving.

## Excitation limits

`analysis_in_flight` is unreachable over one stdio connection by
transport design: the `Admission` gate holds one typed request until its
reply frame flushes, so pipelined followers always observe the completed
snapshot. It is guarded by the pipelined-coherence probe plus the
existing session unit tests that set `in_flight` directly.
`result_too_large` needs a single evidence document over 131072 bytes,
unreachable on this small fixture (the shared budget caps `list_gaps` at
65536 bytes). It is guarded by the advertised-bound pin, the every-frame
audit, and the oversize twin instead of direct excitation.

## Run

```sh
cargo test -p ripr --test agentic_bench_mcp
```

Manual pre-refresh probes against a built fixture root:

```sh
ripr mcp --stdio --root <fixture> < benchmarks/agentic/mcp-journey/b3a.probes.ndjson
```

## Durable resumption and edit recovery (#7463)

The additional real stdio consumers live in
`crates/ripr/tests/support/mcp_attempt_inventory.rs`, using the existing session
and fixture owners. They do not alter the frozen B2/B3 input identity.
The initial consumer reproduction on main
`e57d999ec7427dffd9b92a2a886e8dd53a2ad4e8` completed initialization and discovery,
then failed because the advertised tool list had no durable inventory route.

The durable journey starts with the real B4 Git/Cargo repository. The client
discovers tools, refreshes through the production analyzer, selects a nonempty
changed subject and retrieves its card. The public CLI before phase creates a
canonical durable attempt as fixture setup. A fresh MCP client then discovers
that attempt before any refresh and consumes its ID through both existing
attempt and receipt tools. Literal awaiting/current and diverged/stale results
form the expected-state oracle; inventory reads preserve source, Git and retained
artifact bytes.

Adverse controls retain corrupt rows, reject malformed/null/unknown arguments,
distinguish absent store from unavailable root, and detect empty-store creation.
Unchanged full/narrow/offset reads keep the same inventory identity. Renaming one
invalid directory at unchanged counts rejects an old continuation. Tampering a
real producer artifact refuses its manifest; restoring the original bytes
recovers it. An external committed test edit followed by refresh changes the
snapshot identity, rejects the old snapshot and serves the new one. Ordinary
awaiting-attempt descendants retain shared admission; a diverged HEAD is stale.

A second consumer creates 300 invalid manifests in reverse identity order,
walks automatically byte-limited pages and reconstructs every directory exactly
once. It requires structured content, exact page progress and denominators,
bounded frames and terminal peer reaping even on a failed oracle. Windows uses
plain long names; Unix also exercises JSON-escaped names. Native Windows execution
remains a separate proof dimension. Retained transport cancellation controls own
cancellation semantics; this journey adds no physical-abort or atomic-filesystem
snapshot claim, and output bounds do not imply bounded whole-store validation.

Independent code review also reproduced a near-cap page on exact predecessor
`87921ee008a5a0e4d6f0b4b3b1a5ba2ba8d5a7ce`: 300 invalid rows, the first 70
names with 240 quotes and row 71 with 29. The inner tool envelope fitted, but
JSON-RPC wrapper bytes caused the actual writer to return error `-32603`, losing
structured inventory and continuation. The Rust stdio regression failed on that
same boundary. Inventory fitting now reserves the actual serialized request ID
and SDK/wrapper bytes. The retained counterexample runs with short and escaped
IDs through both legacy initialization and the current inline metadata lifecycle;
it requires complete structured pages and continuing reads, not a generic error.

Remote review identified a second identity counterexample on exact candidate
`9cd2f0136e9e9ed237dfb937c2c36de6032bbf9d`: renaming a malformed manifest's Unix
directory from raw byte `ff` to `fe` preserved the lossy displayed row, summary
digest and accepted continuation. The immutable CLI and actual stdio transcript
retain that original observation. The shared inventory now refuses names it
cannot represent as UTF-8. A public Unix control requires whole-store typed
refusal for both distinct raw names, even when a one-row page would hide them,
then recovery after the external rename to a UTF-8 directory. The control also
pins existing publication semantics: a reserved directory without `attempt.json`
is excluded until the producer publishes its manifest.
