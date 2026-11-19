<!-- section: Fixed -->
- Rust: a related test that checks through a local assertion-like macro ripr
  does not classify (such as a test-module `assert_delta!`) no longer reads
  "has no assertion". The row reads `assertion_admission_unproven` and the
  finding names the test and macro, without granting oracle credit or changing
  the class (#15237).
