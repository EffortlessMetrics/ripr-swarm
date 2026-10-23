<!-- section: Fixed -->
- `baseline diff`, `zero status`, and `first-action` now echo their
  missing-input warnings to stderr (in the `baseline create` warning style) so
  interactive users and `2>&1` CI logs see why a report is empty or limited;
  exit codes and report file contents are unchanged (#9856).
