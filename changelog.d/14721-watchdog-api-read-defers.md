<!-- section: Internal / CI -->
- The PR Staleness Watchdog defers a head whose API read fails (a 502 or a
  non-JSON body) instead of aborting the whole sweep, retries the open-PR list,
  and counts failed reads in its summary (#14721).
