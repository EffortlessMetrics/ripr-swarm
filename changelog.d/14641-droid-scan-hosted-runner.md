<!-- section: Internal / CI -->

- Scheduled Droid security scan: the job now runs on a GitHub-hosted
  `ubuntu-latest` runner instead of the unavailable `em-ci-review` self-hosted
  group, so the weekly Monday run acquires a runner instead of queueing for 24
  hours and being cancelled. It authenticates the Droid action with the job's
  own `GITHUB_TOKEN` rather than OIDC, which is required on a hosted runner now
  that `id-token: write` remains dropped (#14641).
