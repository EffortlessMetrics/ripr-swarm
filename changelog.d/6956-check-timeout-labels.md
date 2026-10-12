<!-- section: Fixed -->
- `ripr check --json` no longer mislabels a stalled Git invocation during
  default-base probing as `base_unresolvable`, or during a candidate-tree
  config read as `config_invalid`. Both paths now emit
  `git_invocation_timeout` with route `analysis/git-timeout`, with the
  timeout diagnostic on stderr echoed verbatim into the envelope message.
  The candidate-tree config read now honors `--git-timeout` /
  `RIPR_GIT_TIMEOUT` (default 300s) instead of a fixed 30s deadline, so
  the documented recovery can actually repair it (#6956).
