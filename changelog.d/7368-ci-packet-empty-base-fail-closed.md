<!-- section: Fixed -->
- `ripr reports ci-packet` no longer composes the literal base `origin/`
  when neither `base_ref` nor `default_branch` is present in the event
  payload (local reproductions, workflow_dispatch, stripped event JSON).
  The first-pr start-here stage now skips with an explicit note naming the
  missing base instead of writing a start-here whose recovery command is
  `git fetch origin -- '+refs/heads/:refs/remotes/origin/'`, a refspec git
  rejects outright. The ci-summary safe-next-action names the same
  condition instead of the hollow base, and first-pr's
  `fetch_base_command` falls back to `git fetch --all --prune` when an
  `origin/` base carries an empty branch (#7368).
