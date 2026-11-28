<!-- section: Fixed -->
- `ripr check` now prints a changed file whose name holds a line break or tab
  on one limitation line, spelling the break as `\n`, `\r` or `\t`, in the
  `file:` field, the recovery text and the withheld repair-start reason. The
  name's tail no longer prints as its own line that reads like `ripr` output.
  Ordinary paths print unchanged (#16455).
