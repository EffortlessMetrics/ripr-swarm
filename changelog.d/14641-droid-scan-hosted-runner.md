<!-- section: Internal / CI -->

- Scheduled Droid security scan: the job now runs on a GitHub-hosted
  `ubuntu-latest` runner instead of the unavailable `em-ci-review` self-hosted
  group, so the weekly Monday run acquires a runner instead of queueing for 24
  hours and being cancelled (#14641).
