# RIPR-SPEC-0206: Batched candidate-tree materialization

Status: proposed

Issue: #5015

## Problem

`git_candidate_execution::materialize` extracted a candidate tree with one
sequential `git cat-file` subprocess per file. For an N-file tree that is N
sequential spawns (tens of ms each on Windows), no progress or phase
disclosure, and — because every per-blob call carried the full
per-invocation git deadline — a worst case of N × git_timeout for a slow or
hanging git. Each call also buffered up to 512 MiB in memory per blob.

## Behavior

- Materialization uses ONE streaming `git cat-file --batch` process for the
  whole tree (two git spawns total with the `ls-tree` listing), regardless
  of tree size. The materialization phase is O(1) in git processes.
- The session is a lockstep request/response protocol: one object ID is
  queued per request, the response header is parsed, and the announced
  bytes stream to the destination file in bounded chunks — a large blob can
  never deadlock against an unread pipe, and at most one 64 KiB chunk is
  resident in memory.
- A single named overall deadline (the same value the per-blob calls each
  used before, enforced once) bounds the entire materialization phase
  incrementally on every blocking read. Deadline expiry classifies as the
  shared named `git_invocation_timeout` error with the same repair route as
  every other git invocation; cooperative cancellation is checked per
  request and per chunk.
- Fail-closed behavior is preserved exactly: unsupported entry modes,
  non-UTF-8 paths, traversal attempts, malformed batch framing, truncated
  streams, and missing objects are named errors; any failure fails the
  whole subject (the temp-root guard removes the partial tree), so no
  partial tree is ever presented as complete.
- Bounded totals: the materialized byte count is capped at
  `MAX_ARCHIVE_BYTES` (512 MiB) for the whole tree (previously a per-blob
  cap), and the `ls-tree` listing keeps its existing per-invocation cap.
- Blob bytes remain byte-identical to per-blob `git cat-file` output: the
  batch protocol returns raw blob bytes, preserving the #3548
  no-textconv identity argument against `git archive`.

## Required Evidence

- Real-repository session round trip: one process answers many blob
  requests byte-identically to the raw `git show` oracle; a nonexistent
  object reports missing without desynchronizing the stream.
- Stream parser unit tests: chunk-boundary reassembly of headers, content
  with interior newlines, and framing; truncated-stream fail-closed;
  spent-budget timeout classification.
- Zero-budget session control: the first blocking read classifies as the
  named `git_invocation_timeout` with the raw prefix intact.
- Scale fixture: a 300-file nested tree plus a >1 MiB binary blob and an
  empty file materialize byte-identically to the `git show` oracle through
  the batched path.
- Existing byte-parity, attribute-conversion, long-path, worktree-isolation,
  and pipeline tests keep passing over the batched path.

## Required guards

- The batch session spawns through the shared git spawn authority
  (`git_command`, `OwnedProcess`): same config hardening, same
  process-tree termination, same error families.
- The overall deadline is enforced on every blocking read; no per-blob
  full-deadline multiplication survives.
- Performance claims stay analytical: spawn count N+1 → 2 is structural;
  wall-clock magnitude remains a measured-future claim (the issue itself
  classifies timing as `design_question`), and no unmeasured speedup is
  asserted in tracked files.
