<!-- section: Changed -->
- Seam inventories fold the corpus `files_content_hash` into the
  single streaming index build instead of reading every Rust file
  once to hash it and again to index it, but only where no warm
  cache hit can be made to pay for that build: once a fingerprint
  key has already proved the classified cache misses, or on entry
  points (diff-scoped prepare, changed-test) that attempt no cache
  load before building. With no fingerprint-derived key (always on
  Windows) the full and compact paths keep the legacy order - hash,
  cache load, build only on miss - and the counts path, which has no
  fingerprint key, does the same, so warm hits pay only the hashing
  read. Cache keys and seam output are byte-identical (#8643).
