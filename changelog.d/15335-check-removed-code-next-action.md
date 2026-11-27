<!-- section: Changed -->
- When a `ripr check` diff only removes code, the `Safe next action` line now
  says that removed code is never an edit target and gives the static
  evidence on the removed lines (for example `1 partly noticed`), instead of
  telling the reader to inspect the run "before treating this run as
  actionable". Runs with moved or unresolved lines keep their wording
  (#15335).
