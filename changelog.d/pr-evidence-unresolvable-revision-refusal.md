<!-- section: Fixed -->
- `ripr pr-evidence` with a `--base` or `--head` that does not resolve to a commit now names the flag and the next step instead of printing `bad base/head revision` with a Debug-formatted git argv and empty stdout/stderr blocks. Outside a git repository it says so and points at `--root`. Exit `2` is unchanged (#14076 follow-up).
