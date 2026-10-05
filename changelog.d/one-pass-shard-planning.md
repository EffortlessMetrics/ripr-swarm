<!-- section: Changed -->
- Storing a large classified-seam cache plans its shards in one pass instead
  of re-encoding growing prefixes about a dozen times per shard (#5364). On a
  cold `ripr pilot` of ripr-swarm (10,000 seams, 20 shards) the cache-store
  phase fell from about 38 s to about 14 s and the whole run from about 87 s
  to about 60 s. Shard boundaries and outputs are unchanged.
