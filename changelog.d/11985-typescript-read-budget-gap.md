<!-- section: Fixed -->
- TypeScript and JavaScript checks with unread workspace sources under a per-file
  or total byte cap now withhold `no_static_path` as `static_unknown` with the
  named `typescript_workspace_read_limited` limit and read-budget recovery
  guidance. Findings backed by observed tests keep their evidence (#11985).
