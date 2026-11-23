<!-- section: Internal / CI -->
- The Windows advisory subset's fixture producer check expects the eighth
  POSIX-only control that #15915 added (39 tests, 31 passes, 8 pending) and pins
  the pending tests by name, so a refusal lists which control stopped or started
  being skipped (#16120).
