<!-- section: Fixed -->
- Re-running `ripr check` after a finished repair no longer turns that
  attempt's receipt `stale`. The finish no longer binds the module-tree scan
  cache file that every check rewrites in place; a staged copy of it, or any
  later edit to a source, test or attempt file, still makes the receipt stale.
  With `RIPR_CACHE_DIR` set the default cache path stays bound. A receipt
  finished by an earlier ripr that bound the cache file keeps its earlier
  reading (#15060).
