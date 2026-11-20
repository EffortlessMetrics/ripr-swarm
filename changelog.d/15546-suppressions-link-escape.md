<!-- section: Security -->
- Every reader of the repository suppressions manifest (`ripr check`,
  `ripr config validate`, `ripr policy suppression-health` and the workspace
  identity scan) refuses a manifest that resolves outside the repository, for
  example through a committed symlink, naming only the configured path, so
  errors no longer echo keys from a file outside the checkout. The manifest is
  read only as a regular file under the repository metadata TOML cap (#15546).
