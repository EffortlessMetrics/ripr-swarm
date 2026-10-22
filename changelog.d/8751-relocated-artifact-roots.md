<!-- section: Fixed -->
- Doctor's artifact-ignore and artifact-tracking probes, and the pipeline's
  changed-scope exclusion of ripr's own reports and caches, now follow the
  artifact base the writers actually use: a relocated `RIPR_CACHE_DIR` or
  `[perl].cache_dir` inside the repository is probed, named in warnings, and
  excluded from diff intake instead of silently reopening the
  committed-artifacts failure the default `target/ripr` protections guard
  (#8751).
