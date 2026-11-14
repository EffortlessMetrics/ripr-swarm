<!-- section: Fixed -->
- An unchanged Rust file or directory that cannot be read no longer aborts
  `ripr check`. The walk skips it and the run reports a
  `language_scope_unsupported` limitation naming the path, so the analysis
  reads incomplete rather than clean, and the workspace index is treated as
  incomplete because the skipped path may hold a test or a competing owner.
  A changed file that cannot be read still fails as before (#13614).
