<!-- section: Fixed -->
- Python preview no longer reports `exposed` for a call that only adds a
  keyword argument equal to the declared default of every workspace
  definition of the callee (for example `shift(check_imaginary=True, ...)`);
  it reads `weakly_exposed` and says the call binds the same value (#16011).
