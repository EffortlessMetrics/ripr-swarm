<!-- section: Fixed -->
- `ripr mcp` no longer offers a finding that `ripr check` withholds because its
  changed owner carries `#[mutants::skip]`: gap lists, gap and limitation
  reads, repair cards and repair preparation refuse it with the typed
  `owner_opt_out` failure, and the gap list, snapshot, refresh and workspace
  status disclose the withheld count with reason `owner_mutants_skip`. A
  snapshot where every finding is withheld says so and never reads as clean
  (#14138).
