<!-- section: Fixed -->
- `ripr check` on a Rust change that is still uncommitted now prints the
  commit step (`git -C <root> --literal-pathspecs commit ...`) before
  `ripr repair <id>`, instead of offering a start that refuses with "commit
  first". The JSON `finding_routes` repair entry reports `blocked_uncommitted`
  with `uncommitted_files`. The refusal text and the dirty-file check now come
  from one shared owner in `app::repair_attempt` (#13636).
