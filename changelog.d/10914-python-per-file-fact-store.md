<!-- section: Changed -->
- Python diff-mode `check` now persists each workspace file's extracted facts under `target/ripr/cache/python-file-facts/`, keyed by content, so a warm run serves untouched files without re-parsing them (RIPR-SPEC-0366, #10914). `check -v` reports `python per-file facts: N reused, M recomputed, K stored`.
