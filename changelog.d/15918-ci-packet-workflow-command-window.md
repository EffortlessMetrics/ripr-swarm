<!-- section: Security -->
- `ripr reports ci-packet` prints each child command's output, stderr
  included, inside a `::stop-commands::` window on its stdout, so a pull
  request file named `x<newline>::error::...` can no longer issue a GitHub
  workflow command from a quoted path in the job log (#15918).
