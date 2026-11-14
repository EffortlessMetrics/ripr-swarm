<!-- section: Fixed -->
- The generated `ripr.yml` now skips the inline review-comment step for fork and Dependabot pull requests, as the summary-comment step already did. Those runs have a read-only token, so the step used to run, fail every request and leave a failed step in the job (#11206).
