<!-- section: Fixed -->
- The managed-server install cache is now bound to the release-admitted manifest
  digest: a cache hit whose receipt records another digest (or none) is a miss
  and re-verifies, and a fresh install rejects a manifest that is not the
  admitted one (effective once a descriptor is admitted; hits are
  offline-trusted until then). A stale `.install.lock` left by a dead process or a heartbeat
  older than five minutes (including a suspended live owner) is reclaimed
  best-effort, lock release and heartbeat are owner-checked, and orphaned `.install-*` and
  `.replaced-*` directories are swept under the lock. Promotion is fenced on lock ownership (best-effort, not atomic), and a cache hit clears leftover `.replaced-*` directories when the lock is free (#9304).
