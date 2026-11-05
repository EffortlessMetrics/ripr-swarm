<!-- section: Performance -->
- Full, compact and counts seam inventories now fold the corpus
  `files_content_hash` into the single streaming index build instead of
  reading every Rust file once to hash it and again to index it on a
  classified-cache miss; cache keys and seam output are byte-identical
  (#8643).
