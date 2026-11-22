<!-- section: Security -->
- `ripr reports ci-packet` prints what each child command sends to the job
  log (its stderr, and its stdout unless the step writes a report file) inside
  a `::stop-commands::` window on its own stdout, so a pull request file named
  `x<newline>::error::...` can no longer issue a GitHub workflow command from
  a quoted path (#15918).
