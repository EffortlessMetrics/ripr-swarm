<!-- section: Internal / CI -->
- `.github/workflows/workflow-lint.yml` runs a path-filtered, repository-wide
  check with checksum-verified jactionlint 2.5.0 and the offline correctness
  profile; local uv guidance records ShellCheck/Pyflakes exclusions without
  changing required qualification or release checks (#13489).
