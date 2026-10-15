<!-- section: Internal / CI -->
- `gh-pr-status` refreshes the PR after protection lookup and refuses changed or
  unavailable exact head/base identities before publishing current readiness;
  unchanged subjects use the final check/review facts without blanket reruns (#4986).
