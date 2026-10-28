<!-- section: Fixed -->
- Shard verify now accepts each shard's latest artifact for this run at or below the verifying attempt, so a partial "re-run failed jobs" qualifies instead of failing with empty shard reads; the verification context records `verified_attempts` when shards were verified from more than one attempt (#10605).
