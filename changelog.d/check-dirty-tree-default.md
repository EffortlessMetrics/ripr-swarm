<!-- section: Changed -->
- `ripr check` analyzes the working tree by default when it has uncommitted
  tracked changes (a staged or unstaged tracked edit), with or without
  `--base`, exactly as `--worktree` does, including drill-in commands that
  carry `--worktree`. A clean tree, or one whose only changes are untracked
  files, still diffs `<base>...HEAD`. A working-tree read names untracked
  source files its diff cannot contain and points to `git add -N`, and an
  empty working-tree read describes the working-tree diff rather than
  `<base>...HEAD`. The new `--committed` flag forces the old
  committed-history read; the `unanalyzed_working_tree` note now names it.
  Every diff-scoped check output names the analyzed base and head: human
  `base:`/`head:` header lines (`HEAD <sha>` or `working tree (uncommitted
  changes on HEAD <sha>)`), additive JSON `base_commit`, `merge_base_commit`
  and `head {source, commit}`, a leading GitHub `ripr analyzed` notice, and
  the same fields in SARIF run properties. `--diff` file/stdin output is
  unchanged. The LSP and the `check_workspace*` library entry points keep
  their existing diff sources (RIPR-SPEC-0116 amendment;
  [#5997](https://github.com/EffortlessMetrics/ripr-swarm/pull/5997)).
