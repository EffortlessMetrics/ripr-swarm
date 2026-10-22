<!-- section: Changed -->
- Classified-cache store planning overlaps its encode checks with the next
  shard's size model and runs the single-entry fit probe while the shard plan
  is computed. On a cold pilot of this repository the cache store phase
  measured 14.2 s before and 9.8 s after (one run each, 4-core container);
  shard ranges and the single-entry decision are unchanged (#5363).
