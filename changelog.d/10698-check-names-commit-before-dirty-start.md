<!-- section: Fixed -->
- `ripr check` and the `agent status` pilot-routing warnings now name the
  `git -C <root> --literal-pathspecs commit` step, with quoted root-relative
  files, before a gap-bound repair start that a dirty tree would refuse, so the
  step pastes and works from any working directory (#10698).
