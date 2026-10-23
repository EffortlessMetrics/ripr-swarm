<!-- section: Fixed -->
- Tolerate GitHub-hosted runner image drift in the repo-policy shell tests:
  the bad-interpreter `unavailable` case accepts both bash exit codes 126 and
  127, and the stubborn-descendant timeout probe waits a bounded window from
  process start (still well inside the witness's natural expiry) so slow
  process-group signal delivery no longer flakes, while both tests keep
  catching a genuinely broken stub or kill path (#9907).
