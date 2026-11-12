<!-- section: Fixed -->
- Path and artifact-shape failures now say what went wrong in plain words,
  name the flag and path, and give the next step instead of a raw
  `os error N` or serde type name: `swarm ingest --result` that is not found,
  `cache export --out` with a missing parent or an existing directory,
  `pilot --out` naming a file, `agent verify` snapshots that are not a
  repo-exposure or check-findings JSON object, and directories passed to
  `outcome --before/--after` or `calibrate cargo-mutants` (#14313).
