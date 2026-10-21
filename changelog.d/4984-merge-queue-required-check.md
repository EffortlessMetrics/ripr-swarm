<!-- section: Internal / CI -->
- The required `Ripr Rust Small Result` now runs once per merge-queue entry (`merge_group`) on the exact queued merge commit instead of on the `merge-ready` label; pull-request activity requests no required work, and repo-policy rejects a re-added `pull_request` trigger (#4984).
