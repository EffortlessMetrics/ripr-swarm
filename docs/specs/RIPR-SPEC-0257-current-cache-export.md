# RIPR-SPEC-0257: Current-input cache snapshot export

Status: proposed

Owner: product-analysis

Created: 2026-10-12

Linked issues: [#7517](https://github.com/EffortlessMetrics/ripr-swarm/issues/7517).

Support-tier impact: None

Selection changes retained cache membership, not analysis coverage, evidence
classes or support tiers.

Policy impact: no dependency, network, workflow or security-policy changes.
Implicit root resolution retains the existing bounded Git child probe; export
does not introduce a new resolver or analysis process.

## Problem

Copying a live cache carries historical workspace keys, old per-file content
keys and unreferenced shard generations into every restored snapshot. A caller
cannot safely select current entries by interpreting the producer's private
formats. The producer needs a read-only export using its ordinary key authority.

## Behavior

`ripr cache export [--root PATH] --out NEW_DIRECTORY` copies a restorable current
cache payload from `RIPR_CACHE_DIR`, or the ordinary workspace cache. Its parent
must exist; an existing destination is refused. Success prints the versioned
JSON report documented in [OUTPUT_SCHEMA](../OUTPUT_SCHEMA.md).

Explicit roots retain the same spelling as ordinary `check`; default roots use
its implicit workspace resolution. Canonical paths serve filesystem guards only.

Selection uses ordinary generated-source discovery, the full workspace key and
per-file content/build keys. Both classified families use that workspace key;
their separate directory schemas remain producer-owned. Export hashes source
bytes directly and never writes a fingerprint mapping into the live cache.

Single-entry representation retains precedence over sharded fallback. Copy only
the selected manifest and its listed shards, plus existing current file facts.
Decode and digest validation use ordinary loaders on copied bytes. At least one
current classified family must be available. Corrupt present families, missing
referenced shards, unsafe references, wrong keys or corrupt present file facts
refuse success. Missing optional file facts are counted explicitly and can be
recomputed by the next analysis.

Exclude historical keys, obsolete schemas/builds, unreferenced generations,
fingerprint mappings, locks, staging files and unrelated siblings. Membership
is bounded by current paths and generations, not mtime or a fixed byte estimate.
Formats and analysis limits remain unchanged. Recheck source/config key identity
after copying before success.

## Ownership and restoration

The live cache is read only. Copies are independent files, never hardlinks.
Existing output, cache overlap and linked entries/ancestors (including Windows
reparse points) are refused. Checks assume cooperative directory ownership;
they do not protect against hostile concurrent directory replacement. An
ordinary publisher removing a referenced file can make export fail safely.

The destination is an exclusive task-owned directory. Require native exit zero
before restoring or publishing it. Failures after output creation retain and
name the incomplete output; preflight refusals, including unavailable Cargo
input identity, occur before the output directory exists and create no output.
There is no successful partial-export result or automatic deletion. Output is
not an atomically published shared cache. Restore a completed directory through
`RIPR_CACHE_DIR` under the same workspace path, analyzer build and applicable
input identity. Omitted fingerprint mappings cost a source hash pass, not loss
of eligible classified/fact reuse.

## Required Evidence

The public command regression changes a Rust predicate three times, requires two
current entries at each export, and checks unchanged source paths and bytes.
Component fixtures preserve both families and complete manifest-listed shards,
admit current keys while refusing historical ones, and reject missing, corrupt,
wrong-key and escaping references. They count absent optional facts and refuse
corrupt present facts. CLI guards refuse overwrite and source overlap.

Frozen real-workspace qualification records runtime, sampled peak and retained
allocated bytes separately from logical encoded lengths and file counts. This
bounds historical membership, not total bytes, process memory or runtime. Full
loader decoding remains unchanged; its read-side bound belongs to #5124. This
contract authorizes neither shared live-cache maintenance, release publication
nor a consumer's private-format pruning.

## Non-Goals

This command does not collect live-cache garbage, enforce a fixed byte or memory
limit, change analysis coverage, publish a shared cache atomically, protect
against hostile directory replacement, or change releases and dependencies.

## Acceptance Examples

- After three predicate revisions, export retains the current classified entry
  and file fact while the live source retains historical entries unchanged.
- A sharded current entry retains its manifest and every listed shard, while an
  unreferenced generation and a historical workspace key are excluded.
- A missing referenced shard, corrupt present fact, linked path, overlapping
  output or existing destination refuses success; the caller cannot restore a
  partial result as a successful snapshot.
- Restoring completed output through `RIPR_CACHE_DIR` under the same root and
  input/build identity preserves ordinary analysis output and eligible cache hits.

## Test Mapping

- `cli::commands::cache::tests::export_current_snapshot_through_command_preserves_source_and_bounds_history`
  covers current selection, source-byte preservation and output guards.
- `cli::commands::cache::tests::export_preserves_relative_analysis_root_identity`
  binds the ordinary relative-root key rather than a canonicalized substitute.
- `cli::commands::cache::tests::export_parser_requires_one_destination_and_keeps_its_flags_separate`
  covers the public argument boundary.
- `cli::command_metadata::tests::production_table_is_complete_and_consistent`
  and `cli::help_json::tests::production_document_is_valid_and_nonempty` cover
  the command's governed machine-help registration.
- `analysis::seam_cache::export::tests::snapshot_keeps_current_manifest_references_and_file_facts_without_history`
  covers both classified families, shard references and historical exclusion.
- `analysis::seam_cache::export::tests::snapshot_refuses_missing_corrupt_wrong_key_and_escaping_shards`
  and `snapshot_counts_missing_optional_facts_and_refuses_corrupt_present_facts`
  cover copied-byte validation and optional-fact accounting.

## Implementation Mapping

- `crates/ripr/src/analysis/seam_inventory.rs::current_cache_keys_at` reuses the
  ordinary corpus/workspace/per-file key builders without fingerprint writes.
- `crates/ripr/src/analysis/seam_cache/export.rs::export_current` owns confinement,
  selection, independent copying, loader validation and the final identity check.
- `crates/ripr/src/cli/commands/cache.rs` owns argument parsing and success JSON.
- `crates/ripr/src/cli/command_metadata.rs` registers the command's machine-help
  effects, output role, cost and exit contract.
- `docs/OUTPUT_SCHEMA.md` defines logical-byte and entry-count report semantics.

## Metrics

Report copied files, present and absent file facts, classified families and
logical encoded file bytes. Qualification records inode-deduplicated allocated
bytes including directories, sampled peak, retained state and elapsed time for
each named workflow. These are workload measurements, not universal growth
bounds; export staging and retained snapshot size are distinct scopes.
