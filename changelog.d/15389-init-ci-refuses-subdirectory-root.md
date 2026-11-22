<!-- section: Fixed -->
- `ripr init --ci github|gitlab --root <subdir>` now refuses (exit 2) when the
  root is below the Git top level, instead of planning a workflow under the
  subdirectory that GitHub and GitLab never read and whose `--root .` commands
  would analyze the wrong directory; the error names the repository-root
  command to run (#15389).
