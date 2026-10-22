<!-- section: Fixed -->
- The managed-server install cache is now bound to the release-admitted manifest
  digest: a cache hit whose receipt records another digest (or none) is a miss
  and re-verifies, and a fresh install rejects a manifest that is not the
  admitted one. A stale `.install.lock` left by a dead process or a heartbeat
  older than five minutes is reclaimed, and orphaned `.install-*` and
  `.replaced-*` directories are swept under the lock (#9304).
