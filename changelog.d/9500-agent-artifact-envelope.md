<!-- section: Fixed -->
- `agent verify`, `agent receipt` and the other consumers that share the
  repo-exposure artifact validation now refuse an artifact envelope whose
  `producer.version` is not the running executable's, even when its content
  commitment is self-consistent. The check is exact-version equality:
  artifacts, receipts and ledgers produced by another RIPR build, even a patch
  release, must be regenerated rather than trusted. The legacy-artifact
  refusal's regeneration command now runs the executable that issued the
  refusal (and prints its version line) instead of a bare `ripr` that a stale
  `PATH` binary could satisfy (#9500).
