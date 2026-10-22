<!-- section: Fixed -->
- `ripr reports gap-ledger --repo-exposure` now refuses an unenveloped legacy
  artifact, or one whose `artifact.producer.version` is not the running
  executable's, with a `blocked` ledger and the same regeneration diagnosis
  `agent verify` gives, instead of ledgering it as a clean `advisory` with
  `warnings: []`. `agent verify` (and receipt/repair consumers) now also refuse
  an envelope whose `producer.version` differs from the running executable even
  when its content commitment is self-consistent; the check is exact-version
  equality, so an artifact from another RIPR build is regenerated rather than
  trusted. The legacy-artifact recovery command now runs the executable that
  issued the refusal (and prints its version line) instead of a bare `ripr` a
  stale `PATH` binary could satisfy (#9500).
