<!-- section: Fixed -->
- `ripr check` on a TypeScript workspace now persists per-file facts (owners, export facts, phase-2 facts and test extractions) in a content-keyed store under the seam-cache base, so a warm run over unchanged input serves untouched files from the cache instead of re-parsing the tree. `-v` reports the real counts (`typescript per-file facts: N reused, M recomputed, K stored`).
  - On zod 680d19f7 (`--base HEAD~1`, 4 cores), a warm check took 1.3–1.4 s against 2.6 s for main aaa12f6. 501 files were reused, 21 barrel files were recomputed, and cold and warm output were byte-identical.
  - A file that re-exports through a relative path is recomputed every run, because its targets resolve on disk.
  - A parse-worker failure is never persisted, a same-length mtime-preserving rewrite still misses, and a corrupt entry is recomputed rather than served (#10725).
