# RIPR-SPEC-0366: Python diff-mode check persists per-file facts keyed by content; a warm run serves untouched files from the store

Status: proposed

Owner: product / analysis

Created: 2026-10-29

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #10914

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Static Python preview stays advisory. This spec governs how
  facts are produced and reused, never what a finding claims. Claim
  boundaries remain governed by [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` (via
  `policy/doc-artifacts.d/`) and `.ripr/traceability.d/`.
- Adds a new cache layer, `python-file-facts/{schema_version}/`, under the
  seam-cache base. The envelope schema version bumps on any change to a
  serialized fact spelling.

## Problem

`ripr check` in a Python workspace extracts source facts from every workspace
file on every run. On pandas (1,545 files) that is about 1.6 s of a 5 s warm
check on 4 cores, and a second run over unchanged input pays it again
(#10914). [RIPR-SPEC-0363](RIPR-SPEC-0363-typescript-per-file-fact-store.md)
gave TypeScript a content-keyed per-file store and named the Python twin as a
known limit. This spec closes that limit.

## Behavior

### The store

Diff-mode Python analysis persists, per workspace file, the facts
`extract_source_facts` derives from it. One envelope per file lives under the
seam-cache base at `python-file-facts/{schema_version}/`.

The envelope does not include the per-statement fact list. Diff mode never
reads it, and it was most of a naive envelope: 273 of 489 MB on pandas.

Values that owners and tests of one file repeat are stored once per distinct
value, each with a per-item index:

- each owner's imports;
- each test's imports;
- each test's constant-rebinding facts.

A table whose index list does not cover exactly the file's owners or tests,
or that names a missing value, is refused as corrupt.

### Content keying, never a stat tuple

An envelope's filename and embedded identity bind:

- the schema version;
- the analyzer identity;
- the normalized repository-relative path;
- a hash of the source bytes.

The workspace-root hash is an envelope-only admission check. No mtime, ctime
or inode participates. Extraction reads nothing but `(path, source)`, so equal
keys mean equal facts. Parse-budget refusals and parse errors persist like
clean files, because they are functions of the same bytes.

A path that the normalized spelling cannot represent losslessly is neither read
from nor written to the store, and is extracted every run. On Unix, that means
a non-UTF-8 name or a name containing a literal backslash.

### Integrity and admission

Every envelope carries the seam_cache typed integrity digest over its identity
and served fields.

- A truncated, mismatched or unsigned entry fails validation. It is
  corrupt-ignored and recomputed, never served.
- The digest is not keyed, so it detects corruption but does not authenticate
  provenance.
- The not-a-directory and Git-tracked-entry probes refuse an unusable base
  once per run. When they do, nothing is read or stored.
- Storing is bounded per run by the encoded envelope bytes (256 MiB).
  Exhausting the budget only skips writes.

### Warm-run semantics and disclosure

- A hit serves the file's facts to the same fold that a fresh extraction
  feeds, so the analysis after the facts loop is unchanged.
- A miss or a corrupt entry extracts and persists.
- In debug builds, every hit is re-extracted and asserted equal to the served
  facts.
- `check -v` prints one line with the real admission counts, in the shared
  preview fact-cache wording:
  `python per-file facts: N reused, M recomputed, K stored`.

## Required Evidence

The fact-store tests:

- The stored form restores every interned field exactly.
- A table that does not cover its items is refused.
- The store serves by content: an edit misses, and a flipped byte is never
  served.
- A cold run stores every file. A warm run serves every file and produces
  findings identical to the cold run. An edit to one file recomputes only that
  file.

## Known Limits

- Repo-exposure mode does not consult the store. It skips Python by design
  (`python_diff_first`).
- A warm run still reads and hashes every workspace file and rebuilds the
  cross-file indexes. The store removes the parse, not the walk.
- A cold run pays the envelope writes: about 1 s on pandas, and a 132 MB
  cache.
- The store is workspace-root-scoped. A cache copied between roots reads as a
  miss.

## Non-Goals

- Runtime mutation proof or coverage claims.
- A generic cross-language fact-store framework.
- Persisting cross-file indexes or post-facts passes.

## Acceptance Examples

1. Two consecutive `check` runs over an unchanged Python workspace: the first
   stores an envelope per file, and the second reuses every file and produces
   identical findings.
2. A one-line edit to one file: that file is recomputed and re-stored, and
   every other file is reused.
3. Flipping one byte inside a cached envelope: the entry is never served, and
   the file is recomputed.
