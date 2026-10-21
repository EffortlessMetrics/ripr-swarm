<!-- section: Fixed -->
- The workflow `ripr init --ci github` generates now passes the pull-request
  number, head SHA, and repository into step `env:` and quotes those values
  from the shell, so GitHub expressions are no longer interpolated into
  `run:` script text (#4452).
