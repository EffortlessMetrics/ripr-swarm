<!-- section: Fixed -->
- Cache: after a run that writes new file facts, ripr removes cached file-fact entries left by older ripr builds, keeping the current build and the two most recently used other builds, so upgrades no longer grow the cache without bound (#7047).
