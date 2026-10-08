<!-- section: Fixed -->
- The PR staleness watchdog report now carries each PR's `mergeable_state`,
  so a conflicting (`dirty`) dark head — which silently gets zero
  `pull_request` runs because the merge ref does not exist — is
  distinguishable from a dropped event delivery at a glance, and `docs/CI.md`
  names merge conflicts as the first check before re-toggling Draft -> Ready
  (#7079).
