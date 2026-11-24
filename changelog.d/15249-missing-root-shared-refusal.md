<!-- section: Changed -->
- A missing or non-directory `--root` is now refused with the shared "<command> root X is not a directory" wording (exit 2) before any work or write, naming the command actually run, for `agent repair`, `diff`, `pr-review front-panel`, `first-pr`, `start-here`, `gate check`, `reports ts-false-actionable`, `receipt write` and `evidence-health` (#15249).
