<!-- section: Internal / CI -->
- `xtask windows-advisory-summary --expected-targets` now takes the workspace
  build's log and its captured exit status, and uses the build's executable
  list only when that status is `0`. A build stopped by its deadline or a
  compile error lists fewer test executables, so a split sample that skipped
  the unlisted targets could pass the completeness check; it is now incomplete
  evidence with the build's status named (#14083).
