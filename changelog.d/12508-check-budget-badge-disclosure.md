<!-- section: Fixed -->
- `ripr check --timeout` help, stderr disclosure, and `analysis_timeout` refusals now distinguish pre-worker git work and post-analysis rendering from git-backed diff loading inside the analysis worker, and name `RIPR_GIT_TIMEOUT` as the per-invocation git bound (#12508).
