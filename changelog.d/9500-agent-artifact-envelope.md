<!-- section: Fixed -->
- `agent verify` (and the receipt/repair consumers that share its repo-exposure
  validation) now refuses an artifact envelope whose `producer.version` is not
  the running executable's, even when its content commitment is
  self-consistent; the check is exact-version equality, so an artifact from
  another RIPR build is regenerated rather than trusted. The legacy-artifact
  refusal's regeneration command now runs the executable that issued the
  refusal (and prints its version line) instead of a bare `ripr` that a stale
  `PATH` binary could satisfy (#9500).
