# Fixture: scope_honesty_unanalyzed_worktree_empty

Spec: RIPR-SPEC-0108

## Given

A byte-pinned `ripr check --base HEAD --json` result has zero findings while a
tracked working-tree edit was not included in the analyzed committed diff.

## When

```bash
cargo xtask check-evidence-promotion-honesty
```

## Then

The semantic corpus must treat the empty result as not clean because the result
emits `unanalyzed_working_tree: true`.

## Must Not

- Treat an excluded dirty working tree as a clean analyzed result.
- Claim that this fixture adds worktree diff analysis.
