<!-- section: Internal / CI -->
- The Scratch GC workflow no longer fires every half hour. No self-hosted pool has accepted its jobs since 2026-11-09, so each fire queued for 30 minutes and was cancelled by the next; it stays available through manual dispatch and the timer is documented for restoring with the pools (#4984).
