<!-- section: Internal / CI -->
- The sharded Windows lane now runs with `CARGO_BUILD_JOBS=2` and `--test-threads 2`. Zero hosted-runner wedges in 80 shard-jobs at this setting, against 12 in 152 (7.9%) at the default; cost is about +9 minutes per shard. Cause of the wedge is still not established (#12338).
