<!-- section: Internal / CI -->
- The `ci` nextest profile's 20-minute hang window is now measured against the
  slowest legitimate test (210s) and pinned to the shard step's 40-minute cap:
  a check fails if the window is widened past it, so a hung test is named by
  nextest instead of ending at the step cap (#14906).
