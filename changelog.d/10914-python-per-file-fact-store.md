<!-- section: Changed -->
- Python diff-mode `check` now persists each workspace file's extracted facts under `target/ripr/cache/python-file-facts/`, keyed by content, so a warm run serves untouched files without re-parsing them. `check -v` reports `python per-file facts: N reused, M recomputed, K stored` (RIPR-SPEC-0366, #10914).
  - Measured on 4 cores against main 0dd7979 (`--base HEAD~1`, draft), with byte-identical output cold and warm:
    - pandas ecdb4258: warm 5.2–5.4 s → 3.7–4.0 s; cold +~1 s (6.2–6.3 s) to write a 135 MB store.
    - Django 5c13b5a: warm 3.4–3.6 s → 2.3–2.4 s; cold 4.3–4.6 s to write a 90 MB store.
  - The envelope leaves out the per-statement fact list diff mode never reads, and stores imports and rebinding facts repeated across a file's owners and tests once.
