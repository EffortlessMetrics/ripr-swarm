<!-- section: Fixed -->
- An empty `base..head` diff in `ripr first-pr` preflight now names the real
  cause (work committed on the base branch itself) and points at the real PR
  base (for example `--base HEAD~1`) instead of a recovery command that
  re-created the same empty diff forever; the no-action/empty-diff headline
  and CLI summary defer to a `needs_attention` preflight instead of declaring
  "stop on no-action" while preflight disagrees (#7372).
