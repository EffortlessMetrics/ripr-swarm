# RIPR-SPEC-0431: Complete Stub-Guidance Reuse

Status: accepted

Owner: analysis

Created: 2026-11-08

Linked issues:

- [#11358](https://github.com/EffortlessMetrics/ripr-swarm/issues/11358)
- [#8547](https://github.com/EffortlessMetrics/ripr-swarm/issues/8547)

## Problem

Repeated `ripr test-stub` guidance for one Rust file rebuilt the workspace
index and re-evaluated every seam each time, although nothing it read had
changed. Reusing the result is only safe if a changed or unverifiable input can
never replay an obsolete test or source identity.

## Behavior

`inventory_file_guidance_at_with_config` returns the same complete classified
inventory as the ordinary diff-scoped wrapper for one file, and may serve it
from the bounded memory or persisted cache when all of the following hold:

- reuse is opted in only for the canonical file scope: no staged consumer, no
  generated context and no repository seam-limit walk;
- the shared workspace key is reusable, so it binds the root, analyzer,
  configuration and the complete consumed corpus content;
- the root and every corpus path round-trip through that key exactly. The key
  folds `to_string_lossy` text and rewrites `\` to `/`, so a non-UTF-8 name, or
  a backslash where it is not the platform separator, can alias another path
  and the result is recomputed instead of reused. A native Windows separator
  folds without loss and stays reusable;
- the guidance identity is its own namespace
  (`file-guidance-v3-lossless-key`) over the file's native path bytes, so it
  cannot collide with a staged or earlier guidance identity;
- on a hit the corpus bytes are re-read and verified unchanged, cancellation is
  honoured, and the invocation reports no file-fact work.

A cache failure, an ineligible identity or an unrepresentable corpus computes
the canonical result. A cache hit grants no repair authority: classification,
oracle and repair decisions stay with their existing owners.

## Required Evidence

Analysis controls pin memory and disk reuse with zero index and evidence work,
invalidation on test edits, additions and removals, key binding of root,
selection, source and configuration, corrupt, missing and unwritable caches,
unverifiable Cargo identity, source drift on hits and cancellation at a hit.
Path controls pin that native spellings do not alias, non-UTF-8 bytes are
preserved, a corpus holding a backslash-named file never replays old advice,
and that the Windows separator rule holds on every host. An app-level control
pins reuse and refresh through the stub consumer.

## Known Limits

- Native Windows execution of the reuse controls is not observed from Linux.
- A persisted entry's own disk read is outside the file-fact stats.
- Equivalent path spellings may conservatively miss; they never alias.

## Non-Goals

- No cold-index redesign and no global cache-key migration.
- No change to staged or MCP caches, caller expansion or seam-limit work.
- No claim of zero total work, atomic filesystem snapshots or release proof.

## Acceptance Examples

- A second guidance request for an unchanged file, tests and configuration
  returns identical guidance without rebuilding the index.
- Renaming `tests/weak.rs` to a Unix file named `tests\weak.rs` keeps the same
  aggregate hash; guidance recomputes instead of replaying the old test path.

## Test Mapping

See `.ripr/traceability.d/11358-complete-stub-guidance-reuse.toml`.

## Implementation Mapping

- `crates/ripr/src/analysis/seam_inventory.rs` (`inventory_file_guidance_at_with_config`, `key_represents_paths_exactly`)
- `crates/ripr/src/app/test_stub.rs` (`inventory_entry_facts`)

## Metrics

- `unit_test_pass_rate`
