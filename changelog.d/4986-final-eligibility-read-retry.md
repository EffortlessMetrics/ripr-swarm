<!-- section: Internal / CI -->
- Routed Rust's final eligibility check retries a bounded transient PR read
  within the same admitted run and attempt, collecting current PR, review,
  thread, queue and succession state again before accepting recovery. Each PR
  read and the complete step have deadlines; semantic refusals and other API
  failures still fail closed (#4986).
