<!-- section: Internal / CI -->
- `.github/workflows/workflow-lint.yml` runs a path-filtered, repository-wide
  check with checksum-verified jactionlint 2.5.0 and the offline correctness
  profile with repository-owned configuration; local uv guidance records
  ShellCheck/Pyflakes exclusions and Node-dependent JavaScript syntax coverage.
  Manual lint dispatches remain independent; required qualification and release
  checks are unchanged (#13489).
