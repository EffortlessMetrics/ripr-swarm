<!-- section: Internal / CI -->
- A manual `Windows Advisory Shards` workflow runs the Windows workspace tests as four nextest partitions plus a doctest job, each well under the single-job step cap, with logs uploaded per shard (#9272).
