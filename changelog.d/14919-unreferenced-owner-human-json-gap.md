<!-- section: Fixed -->
- `ripr check` (human and JSON) now agrees with `--format agent` when a
  changed function is called by nothing in the workspace: `Start here` reads
  "a test gap to inspect or repair" instead of "ripr cannot tell", and the
  JSON next action stops on `top_gap`. Findings ripr genuinely cannot trace
  still read as limits (#14919).
