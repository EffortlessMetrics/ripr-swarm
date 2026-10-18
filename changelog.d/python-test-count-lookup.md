<!-- section: Fixed -->
- Python analysis no longer slows super-quadratically on a file with tens of
  thousands of tests: joining each related test back to its parsed source facts
  is now a name-indexed lookup built once per finding instead of a scan of every
  test per related test. Findings are unchanged.
