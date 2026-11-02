<!-- section: Fixed -->
- `ripr check --timeout` help, stderr disclosure, and `analysis_timeout` refusals now distinguish pre-worker git work and post-analysis rendering from git-backed diff loading inside the analysis worker, and name `RIPR_GIT_TIMEOUT` as the per-invocation git bound (#12508).
<!-- section: Changed -->
- Seam-capped native `repo-badge-json` and `repo-badge-plus-json` outputs carry the `RIPR_REPO_EXPOSURE_SEAM_LIMIT` repair route in `warnings` without duplicating it when report persistence retries (#12508).
