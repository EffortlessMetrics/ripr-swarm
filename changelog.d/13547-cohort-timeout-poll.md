<!-- section: Internal / CI -->
- The cohort process-control test now waits (bounded poll) for the timed-out descendant to reach a dead state instead of racing the reap with one read, so a loaded hosted runner no longer fails shard 2 (#13547).
