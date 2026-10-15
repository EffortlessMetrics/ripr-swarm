<!-- section: Internal / CI -->
- PR evidence prepares one shared analyzer binary and logs separate build,
  canonical snapshot, scoped check and packet timings with snapshot availability
  retained; canonical snapshots and scoped checks share the invoking analysis
  root cache while preserving explicit cache overrides (#8082).
