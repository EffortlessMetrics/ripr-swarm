<!-- section: Fixed -->
- `policy readiness`, `policy waiver-aging`, `pr-comments plan`, `reports index`
  and `reports ci-summary` now refuse a `--root` that does not exist or is not a
  directory with exit 2, through the same check as `ripr check`. They used to
  accept the mistyped path and exit 0, so a run could create a directory tree or
  write into the wrong place (#14548).
