<!-- section: Changed -->
- File-level call facts are derived from per-function calls instead of
  stored as a second copy. Peak `check` RSS falls about 70MB on a
  mid-sized workspace (717MB to 647MB on the #5415 repro, warm cache)
  with byte-identical output, and the on-disk file-fact cache shrinks
  about 60MB; the cache moves to schema 1.22, and legacy payloads
  carrying the removed copy load with derivation authoritative (#5415).
