<!-- section: Added -->
- PR-evidence packets now carry optional `summary.changed_files_sha256` and `summary.changed_files_recipe`, a NUL-framed sorted-deduplicated digest of the changed paths, so an omitted or substituted path is detectable beyond the count. Older packets without the fields still validate (#4006).
