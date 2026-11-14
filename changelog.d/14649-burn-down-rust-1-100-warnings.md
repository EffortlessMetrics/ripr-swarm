<!-- section: Internal / CI -->
- The workspace builds warning-free on both the pinned toolchain and current
  stable. The duplicated `homepage` manifest field is gone, because cargo 1.100
  warns with `redundant_homepage` when it repeats `repository`; `docs/RELEASE.md`
  and `docs/PUBLISHING.md` now state that invariant instead of requiring the
  field. `repo-policy`'s `libc` dependency moved to its Linux target table, the
  `python_env` test fixtures are gated to `cfg(not(windows))`, the doctor
  temp-dir builder no longer needs `let mut` on Windows, and a session-end test
  matches the infallible service error instead of mapping it. The Windows-only
  `clippy::never_loop` finding in the unindexed-member test has a fulfilled
  `#[expect]` under `cfg(not(unix))` with that rationale. Pre-existing clippy
  findings elsewhere in the workspace are
  out of scope for this change (#14649).
