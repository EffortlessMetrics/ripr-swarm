<!-- section: Changed -->
- `ripr check --base HEAD~5` in a repository with fewer commits no longer
  tells you to add a git remote: it says the history is shorter than the
  revision reaches and points at `git log --oneline`. An empty `--base` is now
  named as empty, and `ripr version` suggests `--version` (#5252).
