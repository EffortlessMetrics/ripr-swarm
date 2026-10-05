<!-- section: Fixed -->
- The classified seam cache no longer serves a stale entry after concurrent
  writers: a writer that commits a sharded generation now removes any older
  single entry, so a competing rollback cannot hide the newer generation.
- Generation directories left by a writer that was terminated mid-store are
  removed once they are ten minutes old and no manifest references them. The
  previous manifest is integrity-checked before any of its files are deleted.
- When one classified record (or the cache metadata) exceeds
  `RIPR_CLASSIFIED_SEAM_CACHE_SHARD_BYTES`, ripr now prints one stderr line
  naming the record, its encoded size and the value that restores warm runs.
  Before, every run recomputed with no visible reason.
