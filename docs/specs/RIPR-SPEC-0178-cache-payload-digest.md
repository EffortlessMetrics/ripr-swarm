# RIPR-SPEC-0178: Cache envelope payload digest binding

Status: proposed

Owner:

Created: 2026-09-28

Linked proposal:

Linked ADRs:

Linked plan:

Linked issues:

- #4382 (cache envelopes stored no authentication over their payload: an
  entry whose embedded key fields matched but whose JSON payload was edited
  was served verbatim as evidence on warm runs)

Linked PRs:

- #4591

Support-tier impact:

- No tier change. The digest binding is a cache-integrity property of the
  existing cache layers; every digest outcome degrades to the layer's
  established miss/corruption behavior, so no support claim changes.
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- None. No new policy surface, gate, or allowlist entry; the digest is an
  internal cache-envelope field, not operator configuration.

## Problem

Cache envelopes embedded their key fields and re-verified them on load, but
the served payload carried no authentication: an entry whose payload was
edited after it was written (entity or test renames, sync-tool damage, a
buggy producer) still matched its key on a warm run and was served verbatim
as evidence — on `ripr check --diff`'s repo file-fact layer and on the
classified-seam layers. A wrong stored-evidence signal is worse than a
cache miss: the consumer cannot tell fabricated bytes from analyzed bytes.

Two integrity gaps remained after the initial binding (#4591 review):

1. `CorpusFingerprintEnvelope` — the production `fingerprint ->
   files_content_hash` mapping — carried no digest. Its served hash is the
   root of the repo-seam cache key, so a post-write edit would not merely
   corrupt a payload: it could redirect the whole evidence chain to an
   older, internally-valid classified-seam entry (same configuration
   inputs, digest still valid) and serve stale evidence instead of
   rebuilding.
2. The sharded classified-seam manifest declares `total_seams` outside the
   manifest digest. The loader passed that untrusted value to
   `Vec::with_capacity` before any cross-check, so an oversized or
   overflowing declared count aborted the analysis with a capacity-overflow
   panic instead of degrading to the promised `CorruptIgnored` rebuild.

## Behavior

1. Every production cache envelope binds a SHA-256 digest over its
   serialized served payload at store time and re-verifies it at load time:
   the repo file-fact envelope, the classified-seam envelope, the sharded
   manifest (payload = shard list, limit info, lexical fallback files),
   each shard envelope, and the corpus fingerprint mapping (payload =
   `files_content_hash`).
2. A present-but-mismatched digest is corruption exactly like an
   undecodable entry: the layer reports its typed corrupt/ignored outcome
   with a reason naming the recorded and recomputed digests, and the run
   recomputes and re-stores instead of serving the edited bytes. The
   corpus fingerprint mapping reports `Corrupt`, which its callers decline
   into the honest full path rather than trusting the served hash.
3. Entries written before digest binding (missing `payload_sha256`) are a
   plain miss, never served: the next full run rebuilds and re-stores them
   with a fresh digest. This is a disclosed one-time invalidation, not
   corruption.
4. Manifest-level count fields (`total_seams`, `shard_count`) stay outside
   the manifest digest, but the loader cross-checks them against the
   digest-bound shard list BEFORE any allocation: `shard_count` against the
   listed shard count, and `total_seams` against the checked sum of the
   per-shard seam counts. Any mismatch or overflow is `CorruptIgnored`
   with a reason naming the declared and derived values — never an
   oversized allocation, panic, or abort.
5. The digest is unkeyed and binds integrity, not adversarial
   confidentiality: a writer able to edit payload bytes can recompute it.
   The contract is that accidental or buggy post-write edits, and edits
   that do not bother recomputing, degrade to honest rebuilds.

## Non-Goals

- Authenticated (keyed) cache integrity: the digest detects post-write
  edits and corruption, not a determined adversary who recomputes it; the
  cache directory remains within the workspace trust boundary.
- Forcing byte-identical cache formats across ripr versions beyond the
  existing schema-version fields, or bumping them for the digest
  introduction (`#[serde(default)]` plus the legacy-miss behavior owns the
  transition).
- Caching schema changes for layers outside the envelopes named in the
  implementation mapping.
- Making every legacy entry a typed corruption: pre-digest entries were
  honestly written, so they load as a plain miss and rebuild once.

## Required Evidence

- The stored envelope bytes on disk (each cache layer's on-disk JSON),
  including the recorded `payload_sha256`.
- The recomputed digest over the deserialized served payload at load time.
- The manifest shard list (`index`, `file`, `seams`) as the digest-bound
  source for the `total_seams`/`shard_count` cross-checks.

## Acceptance Examples

1. Warm run over a stored file-fact entry whose test-name payload was
   edited after the write → typed digest-mismatch stderr reason, honest
   recomputed evidence served, entry rebuilt with a fresh digest.
2. Stored classified-seam entry with an edited entity rename and intact
   key fields → `CorruptIgnored` with the recorded and recomputed digests
   named, full recompute instead of fabricated seams.
3. Sharded manifest whose `total_seams` alone is edited to
   `18446744073709551615` → `CorruptIgnored` naming the declared total
   versus the shard-list sum; the run rebuilds — no capacity-overflow
   abort.
4. Stored corpus fingerprint mapping whose `files_content_hash` is edited
   toward an older still-present classified-seam entry → `Corrupt` on
   lookup; the caller declines into the honest full path and recomputes
   the hash from file contents instead of loading the older entry.
5. Any envelope written before digest binding (missing `payload_sha256`)
   → plain miss, one transparent rebuild, digest re-bound on re-store.

## Test Mapping

- `crates/ripr/src/analysis/seam_cache.rs::tests::given_file_fact_payload_edited_when_loading_then_digest_mismatch_degrades_to_rebuild`
- `crates/ripr/src/analysis/seam_cache.rs::tests::given_legacy_file_fact_entry_without_digest_when_loading_then_miss_and_rebuild_serves`
- `crates/ripr/src/analysis/seam_cache.rs::tests::given_classified_seam_payload_edited_when_loading_then_digest_mismatch_degrades_to_rebuild`
- `crates/ripr/src/analysis/seam_cache.rs::tests::given_legacy_classified_seam_entry_without_digest_when_loading_then_miss_and_rebuild_serves`
- `crates/ripr/src/analysis/seam_cache.rs::tests::given_sharded_seam_payload_edited_when_loading_then_digest_mismatch_names_the_shard`
- `crates/ripr/src/analysis/seam_cache.rs::tests::given_sharded_manifest_payload_edited_when_loading_then_digest_mismatch_names_the_manifest`
- `crates/ripr/src/analysis/seam_cache.rs::tests::given_sharded_manifest_total_seams_edited_when_loading_then_corrupt_ignored_not_panic`
- `crates/ripr/src/analysis/seam_cache.rs::tests::given_legacy_sharded_shards_without_digest_when_loading_then_miss_and_rebuild_serves`
- `crates/ripr/src/analysis/seam_cache.rs::tests::corpus_fingerprint_lookup_detailed_rejects_edited_payload_hash`
- `crates/ripr/src/analysis/seam_cache.rs::tests::corpus_fingerprint_lookup_treats_legacy_mapping_without_digest_as_miss`
- `crates/ripr/src/analysis/seam_cache.rs::tests::corpus_fingerprint_lookup_detailed_names_the_miss_kind`
- `crates/ripr/src/analysis/seam_cache.rs::tests::corpus_fingerprint_lookup_detailed_rejects_undecodable_entry`

## Implementation Mapping

- `crates/ripr/src/analysis/seam_cache.rs` — `payload_digest`,
  `payload_digest_status`, `PayloadDigestStatus`; digest fields and
  verification on `CacheEnvelope`, `FileFactCacheEnvelope`,
  `ShardedCacheManifest`, `ShardedCacheEnvelope`, and
  `CorpusFingerprintEnvelope`; `load_sharded_classified_seams` count
  cross-checks before allocation.
- `crates/ripr/src/analysis/seam_inventory.rs` — corpus fingerprint
  mapping consumers decline `Corrupt`/`Missing` into the honest full path
  and re-store the mapping with a fresh digest.
- `docs/CONFIGURATION.md` — the user-facing cache integrity paragraph.

## Metrics

- `cache_payload_digest_mismatch_rebuilds` — gate: all digest-binding
  acceptance tests pass, including the corpus fingerprint mapping refusing
  an edited served hash and every legacy-shape entry loading as a plain
  miss.
- `cache_manifest_count_cross_check_rebuilds` — gate: the oversized
  `total_seams` manifest degrades to `CorruptIgnored` naming the declared
  total versus the shard-list sum, never an allocation abort.
- Promote to accepted when a warm-run repro of the original #4382 tamper
  scenario (edited payload, key fields intact) is confirmed serving
  rebuilt evidence with a typed digest-mismatch reason on every covered
  layer, including the corpus fingerprint mapping.
