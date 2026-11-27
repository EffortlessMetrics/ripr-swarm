<!-- section: Fixed -->
- When the editor's analysis attempt fails before any snapshot exists (for
  example the folder is not inside a Git work tree, or the repository has no
  commits), the `ripr-diagnostic-analysis-pending` marker now says ripr could
  not analyze the workspace, quotes the recorded failure and its fix, and uses
  recovery `fix_failure_then_refresh`, instead of claiming analysis has not
  started and that saving or `ripr.refresh` would start it (#15461, follows #14447).
