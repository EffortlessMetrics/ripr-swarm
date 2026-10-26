<!-- section: Fixed -->
- The staleness watchdog re-reads a merge-ready PR whose mergeability is still being computed (up to three reads, five seconds apart) before deciding, so the conflict report is not skipped right after main moves (#10136).
