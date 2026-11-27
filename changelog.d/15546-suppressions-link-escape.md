<!-- section: Security -->
- Every reader of the repository suppressions manifest (the `ripr check`
  suppression filter, `ripr policy suppression-health` and the workspace
  identity scan) refuses a manifest that resolves outside the repository, for
  example through a committed symlink. Errors name only the configured path,
  so they no longer echo keys from a file outside the checkout, and outside
  bytes no longer feed the workspace identity. The manifest is read only as a
  regular file under the repository metadata TOML cap (#15546).
