<!-- section: Added -->
- `ripr cache status --json` now has a published schema,
  `schemas/ripr/cache-status.schema.json`, including the two cross-field
  rules the docs already stated: `via_symlink` agrees with `real_cache_dir`,
  and a `not_found` or `unavailable` report has every count at `0`. The
  emitted bytes and `schema_version` `0.2` are unchanged (#14067).
