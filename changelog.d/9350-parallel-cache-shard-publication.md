<!-- section: Changed -->
- Sharded classified-cache publication writes shards with up to four workers
  instead of one at a time. On a cold pilot of this repository the cache store
  phase measured 25.5 s before and 17.0 s after (one run each, 4-core
  container); output bytes and error ordering are unchanged (#5363).
