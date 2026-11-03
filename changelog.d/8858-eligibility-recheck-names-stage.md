<!-- section: Fixed -->
- The routed Rust Result's eligibility recheck now annotates the job log with the stage that refused (admission binding, pull request state, review decision and threads, or same-head succession) instead of failing with only an exit code. Which conditions refuse is unchanged (#8858).
