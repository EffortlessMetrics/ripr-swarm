<!-- section: Fixed -->
- When an editor refresh fails because the configured base does not resolve,
  ripr no longer keeps showing the findings from the earlier base as if they
  were current. Pull and push clients get one Information marker that names
  the failure and says the earlier findings are withdrawn; the findings
  return with the next successful refresh (#15312).
