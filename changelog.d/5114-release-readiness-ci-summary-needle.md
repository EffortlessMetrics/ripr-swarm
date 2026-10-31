<!-- section: Internal / CI -->
- The release-readiness `github-workflow-defaults` row now looks for the
  `start-here.md` artifact wording the first-run `reports ci-summary` prints
  since the job summary began naming the artifact, instead of the old
  `target/ripr/reports/start-here.md` path (#5114).
