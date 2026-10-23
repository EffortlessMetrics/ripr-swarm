# PR9890 isolated contribution and portable receipts

Source contribution: `eb1326dc1522d1051f08cddc35cff6ad7ce9f869` on `review/pr9890-closure-attribution-20261023`.
Its parent is the already published `83442db9d7c8350bdff9abcc3241e034f4589d8d`; that commit's parent is canonical `b4f04013c237dd034a5009dbfd46d45bef7e4733`.
Production code, tests and expected outputs equal executed/reviewed `7e4b5b5ddc31d43a0cb5169f07d4e56acb75a8a3`; the final tree additionally repairs only the fixture SPEC's required BDD sections. Exact validated tree and doc patch are recorded in the final run receipt.
This evidence branch is separate from the source adoption range. Do not adopt its evidence commit as product source.

Canonical source custody remains with the existing #9890 author. This contribution is not canonical adoption, hosted qualification, merge clearance or delivery to main.

Latest inspected main is `cc72500387c0fb152d1dbd6b40784f2bd925b42a`. Material owner/assertion-context consumers changed after the executed basis. Read-only composition now has an actual honesty-corpus append conflict. `receipts/repair/main-cc725-integration/` contains exact main/proposal corpus inputs, a verified patch/JSON union retaining all417main cases plus the closure case, and the relevant consumer diff. It is an isolated resolution proposal, not latest-main execution proof; preserve later main additions if that input advances.

The archive contains 2158 UTF-8 evidence files (11643136 bytes before compression), including actual drivers, fixture source/diffs, commands, native statuses, analyzer output and identities. No compiled binary, mutable target or Python bytecode is transferred. Frozen executables remain in the task environment; their hashes are in the observed receipts. PACKET.json and the per-file manifest are the packet inventory. Historical manifests within the archive describe their original local evidence, including binaries that this packet excludes.

## Evidence scope

- Original e184:19 actual CLI/native pairs and public-API behavioral reds.
- Repaired d2:51 CLI/native pairs,52 focused tests,18 exposed positive mutants rejected by assertion101,4 alternate-input witnesses.
- Final7e4:25 API tests;4 refreshed float pairs; original and combined eleven-target mutant each pass11 native tests. The combined experiment is two executables, not eleven independent pairs.
- Companion eight native pairs are explicitly reused after exact source-hash matching; frozen e184/d2 analyzer executions are fresh separately.
- Targeted eleven-case golden generation and independent honesty gate pass at3c4; final7e4 changes only those generated expected outputs.
- Exact-head policy results and any retained non-pass diagnostics are in repair/final-run-receipt.json and raw logs. No full local/hosted CI equivalence is claimed.

Identity clarification: the executed d2/final-float drivers inherited a `merge_base` label6417 from the original e184 review. The actual composed proposal/current-main merge base is97df16; canonical source base isb4. See repair/identity-clarification.json for exact Git verification. This does not affect fixture-root/explicit-diff executions, and raw receipts are preserved unchanged.

Independent review and the acceptance matrix are also available as top-level files. Historical `*-to-contribution.patch` files reverse from the final source tree to exact68be/d2/3c4 trees; historical-reconstruction-final.json records independent tree-hash verification, and source-identities-final.json binds the trees and patch hashes. Earlier reconstruction receipts remain historical. This reconstructs source bytes, not the old commit identity. Building a dirty final checkout could report the final HEAD for historical bytes and must not be labeled an old-commit execution. For a historical rerun, fetch the actual old commit if available, or verify the reconstructed `git write-tree` against source-identities-final.json, create a separately labeled synthetic commit with `git commit-tree VERIFIED_TREE -p eb1326dc1522d1051f08cddc35cff6ad7ce9f869`, and build it in an isolated worktree. Record its new SHA/version as a synthetic reconstruction; never relabel that run d2/68be/3c4. No earlier execution is relabeled as a final-source run.

For an optional d2 source reconstruction, run in the fetched repository using a task-owned absent temporary index (this does not move the current worktree HEAD):

```bash
export GIT_INDEX_FILE=/absolute/path/absent-pr9890-reconstruction.index
git read-tree eb1326dc1522d1051f08cddc35cff6ad7ce9f869
git apply --cached --reverse /absolute/path/receipts/repair/historical-native_51-to-contribution.patch
git write-tree
```

Compare that output with `native_51.tree` in `receipts/repair/source-identities-final.json` before creating any synthetic commit. Use `git commit-tree VERIFIED_TREE -p eb1326dc1522d1051f08cddc35cff6ad7ce9f869 -m 'Synthetic d2 source reconstruction for review'` and record its returned SHA; `unset GIT_INDEX_FILE` before adding an isolated worktree at that synthetic SHA. Its analyzer build must report that new SHA. The original d2 execution remains the historical captured receipt.

## Receiving and replay

Fetch the two review-owned refs and use the immutable source/proof SHAs in the #9890 continuation record. Extract `review-evidence/pr9890/text-receipts.tar.gz` from the proof commit with `git show PROOF_SHA:review-evidence/pr9890/text-receipts.tar.gz > text-receipts.tar.gz`.
Verify and extract the archive:

```bash
set -e
printf '%s  %s\n' 'dcf066cf4fd67c35513efa38c52946d7f4bfa660d9d3b23d01a275c6eff40125' text-receipts.tar.gz | sha256sum --check
tar -xzf text-receipts.tar.gz
python3 - <<'VERIFY'
from pathlib import Path
import hashlib, json
root = Path('receipts')
manifest = json.loads((root / 'PACKET-MANIFEST.json').read_text())
for item in manifest['files']:
    data = (root / item['path']).read_bytes()
    if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
        raise RuntimeError('manifest mismatch: ' + item['path'])
print('verified', len(manifest['files']), 'text evidence files')
VERIFY
```

Create an isolated source worktree with `git worktree add --detach /absolute/path/absent-source-worktree eb1326dc1522d1051f08cddc35cff6ad7ce9f869`. With pinned Rust1.95.0 and admitted resources, run `cargo build --locked -p ripr --bin ripr` inside it and record `target/debug/ripr --version` and `sha256sum target/debug/ripr`. A fresh build may have different bytes; the replay records the receiving binary identity and requires the selected full source revision.

Use the parameterized successor, preserving the original executed drivers unchanged:

```bash
python3 /absolute/path/receipts/repair/replay_portable.py --receipts /absolute/path/receipts \
  --analyzer /absolute/path/source/target/debug/ripr \
  --rustc /absolute/path/rustc --expected-revision eb1326dc1522d1051f08cddc35cff6ad7ce9f869 \
  --out /absolute/path/absent-replay-output \
  --case direct --case captured_offset_zero
```

Use `--all` instead of case names only when a complete51-pair replay is admitted and necessary. This helper replays only the51 captured original/mutant pairs; it does not execute the companion8, combined11-target fixture or4 alternate-input non-equivalence witnesses. Those have separate captured sources/drivers and scoped receipts. The two-case helper smoke result is a handoff-instrument check, not a new51-pair proof. Helper outputs identify each actual source, executable, command and terminal result.

## Next owner action

At unchanged canonicalb4, the existing source owner can fetch the source ref and adopt the two commits in `b4f04013c237dd034a5009dbfd46d45bef7e4733..eb1326dc1522d1051f08cddc35cff6ad7ce9f869`. If source advanced, compare the actual new tree and adopt only applicable changes. Then refresh current-head source/integration review, verify and reply to repaired findings before resolution, request one normal selected-head qualification and inspect its real required jobs. Only an eligible actual canonical head may merge through normal protected expected-head squash; verify main afterward. This packet does not authorize another writer on the canonical branch or a protection bypass.
