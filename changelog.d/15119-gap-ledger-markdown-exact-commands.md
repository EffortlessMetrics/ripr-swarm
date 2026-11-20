<!-- section: Fixed -->
- `ripr reports gap-ledger` shows verify, inspect, regenerate and receipt
  commands in its Markdown report exactly as the JSON ledger carries them. A
  pipe no longer gains a backslash and a backtick no longer becomes an
  apostrophe, so a command copied from the report runs as written. Warnings
  and limits no longer open raw HTML or notify an `@name` account when the
  report is posted (#15119).
