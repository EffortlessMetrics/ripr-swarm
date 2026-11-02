<!-- section: Internal / CI -->
- The sharded Windows lane's job cap is now 105 minutes, above the warm steps plus the 55-minute test step cap, and each shard prints free memory and its four largest processes every minute and uploads them as `shard-N.heartbeat`, so a shard that hangs with no log leaves its last observed state (#12338).
