# RIPR-SPEC-0363: TypeScript diff-mode check persists per-file facts keyed by content; a warm run serves untouched files from the store

Status: proposed

Owner: product / analysis

Created: 2026-10-27

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #10725

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Static TypeScript preview stays advisory; this spec governs
  how facts are produced and reused, never what a finding claims. Claim
  boundaries remain governed by [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` (via
  `policy/doc-artifacts.d/`) and `.ripr/traceability.d/`.
- A new cache layer (`typescript-file-facts/{schema_version}/`) under the
  seam-cache base; envelope schema version bumps on any serialized fact
  spelling change.

## Problem

`ripr check` in a TypeScript workspace re-parsed every workspace file on
every run: a one-line diff beside a 3.15 MB file cost ~48 s, and a second
run over unchanged input paid the same cost again — the cost followed tree
bytes, not diff size (#10725). The in-memory extraction cache (#10071) only
lives for one process, so a CLI invocation — one process per run — never
benefits. The Rust adapter already persists per-file facts keyed by content
(`RepoFileFactCache`); the preview adapters had no equivalent.

## Behavior

### The store

Diff-mode TypeScript analysis persists, per workspace file, the facts one
parse derives: the combined `FileFacts` (parse status, owners, export
facts, phase-2 facts) and, for test files, the pre-promotion test
extraction and its partial-extraction verdict (whether the extractor dropped
a registration shape in that file, recorded only when the check completed).
One envelope per file lives under the seam-cache base at
`typescript-file-facts/{schema_version}/`.

### Size stays linear in the file

A file-wide list that many entries hold in memory (each test's imports and
scope bindings, each owner's imports) is written once per envelope and
referenced by id afterwards, and envelopes are written compact. Without this
a 1.5 MB test file with 767 tests persisted a 267 MB envelope, which spent
the per-run store budget and left a large workspace cold for several runs
(#11676). The wire form is decoded back into the same sharing, and the
integrity digest covers the wire form.

### Content keying, never a stat tuple

An envelope's filename and embedded identity bind the schema version, the
analyzer identity, the normalized repository-relative path, and a hash of
the source bytes. The workspace-root hash and the `tsconfig` alias-map
digest are envelope-only admission checks. No mtime, ctime, or inode
participates: a same-length rewrite with a restored modification time
(`rsync -a`, `cp --preserve=timestamps`) misses and re-parses, mirroring
the per-file cache contract the Rust adapter established (#2175/#3848).
Parse-error facts persist exactly like clean ones — the envelope is
content-keyed, so the cached default facts are as honest as the in-memory
cache's.

### Integrity and admission

Every envelope carries the seam_cache typed integrity digest over its
identity and served fields. The digest detects corruption: a truncated,
mismatched or unsigned entry fails validation, is corrupt-ignored, and is
recomputed, never served. It is not keyed, so it does not authenticate
provenance: whoever can write the cache directory can write a consistent
entry. The not-a-directory and Git-tracked-entry probes refuse to read or
write an unusable base, and the run reports that once instead of a corrupt
entry per file. Storing is bounded per run by the encoded envelope bytes;
exhausting the budget only skips writes, which is correctness-neutral
because the parse already happened.

A file whose export facts resolve a relative re-export (`export * from`,
`export { x } from`, `export * as ns from`) is recomputed every run and
never persisted: the resolution reads the filesystem (a directory's
`index`, its `package.json`, a `tsc` output mapped back to source), which
can change while the barrel's bytes do not. A renamed own export resolves to
the file itself and is persisted. A test extraction is persisted only when
the process-lifetime cache holds it; a parse-worker failure serves an empty
extraction that is never cached or persisted. A process that already holds a
file's facts in memory neither reads nor rewrites its envelope. Storing defers
until after the parse loop, so cache IO never interleaves with parsing.

### Warm-run semantics and disclosure

A disk hit rehydrates the process-lifetime extraction caches, so the
analysis loop is unchanged; a miss or corrupt entry recomputes and queues
the facts for storing. `check -v` prints one line with the real admission
counts — `typescript per-file facts: N reused, M recomputed, K stored` —
only when the run did non-zero cache work; a zero-work run stays silent.
A miss against an entry the file had at run start is disclosed as an
invalidated file. Worker-refused files are never persisted, exactly like
the memory cache.

## Required Evidence

- The six fact-store tests: cold persistence with zero reuse; warm rerun
  reuse with findings identical to a cold run; edit invalidation scoped to
  the changed file; tamper corrupt-ignored and recomputed; mtime-preserving
  same-length rewrite recomputed; the `-v` line wording and its silence on
  zero work.

## Known Limits

- Python diff mode has no persistent store yet; its warm path is a
  follow-up reusing this pattern.
- `ripr doctor` probes the layer's entry directory for writability (it
  is in `production_entry_dirs` when TypeScript is built in) but reports
  no per-layer reuse or entry counts.
- Reuse counts are per-run admissions, not a cross-run history.
- The store is workspace-root-scoped; copying a cache between roots
  changes the embedded root hash and reads as a miss.

## Non-Goals

- Runtime mutation proof or coverage claims.
- Cross-language fact sharing or a generic fact-store framework.
- Persisting promoted (filesystem-dependent) test extraction state.

## Acceptance Examples

1. Two consecutive `check` runs over an unchanged TypeScript workspace:
   the first stores an envelope per file and reports `reused 0`; the
   second reports `reused N` for all N files and produces findings
   identical to a cold run.
2. A one-line edit to one file: that file is recomputed and re-stored;
   every other file is reused; `-v` names the recomputed count.
3. Flipping one byte inside a cached envelope: the entry is
   corrupt-ignored, the file is recomputed, and findings match the cold
   run; the corrupt read is counted, never served.
4. A same-length rewrite with the mtime restored: the file misses (the
   content hash changed) and re-parses.
5. `check -v` on a run that touched no cache work prints no fact-cache
   line.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/fact_store_tests.rs`:
  `cold_run_persists_per_file_facts_for_every_workspace_file`,
  `warm_rerun_reuses_persisted_facts_and_reproduces_identical_findings`,
  `edited_file_is_recomputed_while_unchanged_files_reuse`,
  `tampered_cache_entry_is_recomputed_not_served`,
  `mtime_preserving_same_length_rewrite_is_recomputed`,
  `fact_cache_line_discloses_real_counts_and_stays_silent_on_zero_work`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/fact_store.rs`: the
  content-keyed store, envelope, integrity, probes, and invalidated-file
  attribution.
- `crates/ripr/src/analysis/language/typescript/mod.rs`: Phase-1
  load/store wiring and the stats carrier fill.
- `crates/ripr/src/analysis/language/adapter.rs`:
  `PreviewFactCacheStats` and `render_fact_cache_line`.
- `crates/ripr/src/analysis/pipeline.rs` and
  `crates/ripr/src/app/check.rs`: stats collection and the `-v` line.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
