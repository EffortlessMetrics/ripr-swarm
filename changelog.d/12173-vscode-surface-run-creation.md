<!-- section: Internal / CI -->
- The routed Result now binds the extension e2e verdict to the `ci.yml` run created for the merge-ready request rather than to a job start time, so a slow runner no longer times out a valid request (#12173).
