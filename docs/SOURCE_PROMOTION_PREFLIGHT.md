# Source-promotion preflight

`cargo xtask source-promotion preflight` creates the repeatable, read-only
receipt consumed by the source release preflight. It preserves the complete
swarm history as the selected parent; it does not create the join.

## Command

Run it from a clean operator checkout with both repositories available locally:

```bash
cargo xtask source-promotion preflight \
  --source-parent "$SOURCE_PARENT" \
  --swarm-parent "$SWARM_PARENT" \
  --source-repo ../ripr \
  --swarm-repo ../ripr-swarm \
  --source-main origin/main \
  --swarm-main origin/main \
  --version 0.11.0 \
  --out target/ripr/source-promotion
```

The parent arguments must be complete 40-character commit IDs. The source
parent must equal the declared current source main. The swarm parent must be
reachable from the declared swarm main. Repository roots must be distinct and
their `origin` URLs must identify `EffortlessMetrics/ripr` and
`EffortlessMetrics/ripr-swarm`; use `--source-remote`/`--swarm-remote` only for
an explicitly reviewed mirror.

The command writes deterministic `source-promotion-preflight.json` and `.md`
files. It records the merge base, separately named all-reachable and
first-parent counts for each parent range, and SHA-256 digests. The ordered
digest recipe is:

```text
git rev-list --first-parent --reverse MERGE_BASE..PARENT
UTF-8 SHA lines joined with LF, then SHA-256
```

It also inventories changed paths, source survivors, swarm repository-authority
paths, version/changelog state, and a real `git merge-tree --write-tree` dry
merge. The dry merge runs in a disposable repository populated by fetching both
exact commits. No branch, index, ref, working tree, version, tag, PR, or
publication state in either authoritative checkout is changed.

## Receipt boundary

The receipt is invalid when either parent, declared main, repository identity,
merge base, ancestry count, digest, conflict list, or resolved tree changes.
Regenerate it if `main` moves before the transaction boundary. A clean dry
merge is not proof that semantic overlap is absent; every textual and semantic
resolution still needs review.

This command does not construct or prove the two-parent join, qualify the
candidate, change versions, authorize publication, publish artifacts, or
perform the source-to-swarm back-sync. Those are separate release boundaries.
