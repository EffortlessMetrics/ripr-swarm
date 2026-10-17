<!-- section: Changed -->
- With no analysis scope, `ripr check` human triage and note, the JSON `why`,
  the GitHub warning, the default-base failure and the `ripr doctor` routes
  (not a Git work tree, unborn `HEAD`) now name the same pair: `ripr check
  --base BASE` for a diff and `ripr pilot --root .` for whole-repository
  exploration, instead of `ripr check --format repo-exposure-md`, which stays a
  valid explicit format. The doctor `git_repository` evidence shell-quotes the
  pilot root so a path with a space pastes as one argument. Scripts that match
  the old guidance text should update; no `schema_version` changes (#8483).
