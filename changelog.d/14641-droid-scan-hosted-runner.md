<!-- section: Internal / CI -->

- Scheduled Droid security scan: the job now runs on a GitHub-hosted
  `ubuntu-latest` runner instead of the unavailable `em-ci-review` self-hosted
  group, so the weekly Monday run acquires a runner instead of queueing for 24
  hours and being cancelled (#14641). Separately, #2498 removed the OIDC grant
  (`id-token: write`). The workflow passes the job's own `GITHUB_TOKEN` to
  authenticate the Droid action. `check-droid-review-config`
  now rejects a self-hosted `runs-on` or a missing `github_token`, so neither
  repair can silently regress.
