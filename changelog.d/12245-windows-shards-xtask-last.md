<!-- section: Internal / CI -->
- The sharded Windows lane now runs nextest twice per shard, `--workspace --exclude xtask` and then `-p xtask`, so the `xtask` binary is rebuilt in nextest's build step instead of inside `policy_checker_facade_runs_current_repo_checks`' capped `cargo` spawns. Each run keeps `--partition count:N/4`, so the four shards still cover every test once (#12245).
