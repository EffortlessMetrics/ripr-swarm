<!-- section: Changed -->
- Storing a large classified-seam cache plans its shards in one pass instead
  of re-encoding growing prefixes about a dozen times per shard (#5364). On a
  cold `ripr pilot` the cache-store phase fell from about 38 s to about 14 s
  on ripr-swarm (10,000 seams, 20 shards) and from about 12-16 s to about
  4-5 s on tokio (10,000 seams, 10 shards), taking tokio's first run from
  about 35-39 s to about 24-26 s. Shard boundaries match the previous
  planner, which still decides any shard the size model cannot confirm.
