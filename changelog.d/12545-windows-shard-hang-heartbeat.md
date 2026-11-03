<!-- section: Internal / CI -->
- The sharded Windows advisory lane's nextest watchdog now ends at the earlier
  of 44 minutes and the time left before the 80-minute job cap minus an
  8-minute summarize/upload reserve, and writes a SLOW/TIMEOUT/FAIL heartbeat
  so a shard that would otherwise be cancelled at the runner cap still leaves a
  named in-flight test and an `always()` artifact (#12338).
