<!-- section: Changed -->
- The source-promotion preflight receipt now retains `path_record_count` and a NUL-terminated `path_record_sha256` for the source-changed, swarm-changed and swarm-only path inventories, so an omitted, added or newline-split path changes the retained denominator (#4006).
