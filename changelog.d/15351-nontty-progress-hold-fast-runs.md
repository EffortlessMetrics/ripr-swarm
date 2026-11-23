<!-- section: Changed -->
- Human-format `ripr check` / `ripr explain` on a piped stderr (an agent's shell tool, a CI log) no longer print `ripr progress:` stage lines and `completed` for a run that finishes in under 2s; longer runs still print their stage and heartbeats. `RIPR_PROGRESS=1` restores immediate lines, and `--quiet` / `RIPR_QUIET` still win (#15351).
