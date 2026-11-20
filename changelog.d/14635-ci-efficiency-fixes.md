<!-- section: Internal / CI -->
- Harden `routed-rust.yml` and `rust-gates.yml` CI efficiency and admission:
  enforce review-disposition and unresolved-thread pre-checks on `merge_group`
  admission, use `github.token` (with `pull-requests: read`) for `merge_group`
  PR lookups so an expired `EM_RUNNER_READ_TOKEN` cannot misroute queued PRs as
  untrusted forks, and set `CARGO_INCREMENTAL=0` alongside `sccache`
  (#14200, #14635, #14638).
