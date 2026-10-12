# RIPR-SPEC-0252: Current-input cache snapshot export

Status: proposed

Owner: product-analysis

Created: 2026-10-12

Linked issues: [#7517](https://github.com/EffortlessMetrics/ripr-swarm/issues/7517).

Support-tier impact: none. Selection changes retained cache membership, not
analysis coverage, evidence classes or support tiers.

Policy impact: none. No dependencies, process spawns, network, workflow or
security-policy changes.

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
before restoring or publishing it. Failure retains and names incomplete output;
there is no successful partial-export result or automatic deletion. Output is
not an atomically published shared cache. Restore a completed directory through
`RIPR_CACHE_DIR` under the same workspace path, analyzer build and applicable
input identity. Omitted fingerprint mappings cost a source hash pass, not loss
of eligible classified/fact reuse.

## Proof and limits

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
