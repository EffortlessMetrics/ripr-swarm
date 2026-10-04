# B6 edit-cage provenance binding

Benchmark fixture for the edit-cage provenance binding bench, driven by
`cargo xtask agentic-bench` (report schema `ripr-agentic-bench-v1`) and
oracled by `cargo test -p ripr --test agentic_bench_cage`.

## Stimulus

1. Capture a bounded baseline snapshot of a small worktree (path count,
   per-file bytes, and total bytes all capped; over-budget capture fails
   closed instead of silently truncating).
2. Apply one edit inside the allowed surface (`tests/` subtree, touching
   the selected target `tests/cage_target.rs`).
3. Apply one edit outside the allowed surface (`src/` subtree).
4. Re-read the digest-bound receipt after each step.

## Oracle

- States are digest-bound: a receipt names the snapshot and policy digests
  it was issued against, and re-reading against any other bytes refuses to
  stay `compliant`.
- Out-of-surface edits fail closed (`violated`, never silently compliant).
- A superseded snapshot (re-captured after a change) never validates an
  older receipt; the re-read reports `superseded`, not `compliant`.
- An invalid manifest (malformed JSON, unknown fields, escaping paths, or
  an empty surface) is rejected before any receipt is issued.

## Files

- `manifest.json` (`ripr-agentic-bench-manifest-v1`): bench identity,
  allowed surface, and `sha256:`-bound fixture files. Policy paths are
  relative to a snapshot root; `fixtures[].path` entries are relative to
  this directory.
- `input/`: the snapshotted worktree. The oracle copies `input/*` into a
  temp root, so `input/src/lib.rs` becomes snapshot path `src/lib.rs`.

## Claim boundary

This bench exercises receipt binding mechanics on synthetic fixtures only.
It claims nothing about runtime mutation behavior, real-repository cage
verdicts, or candidate qualification.
