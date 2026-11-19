<!-- section: Added -->
- `RIPR_GENERATED_AT=unix_ms:<millis>` pins `artifact.generated_at` in `check-findings-json`, `repo-exposure-json` and `repo-seams-json`, so two runs over the same inputs write byte-identical artifacts and digests; a malformed value refuses the run instead of stamping the current time (#14230).
