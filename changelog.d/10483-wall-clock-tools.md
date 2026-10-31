<!-- section: Internal / CI -->
- Repo-policy and xtask process tests (FIFO inspection, descendant termination, deadline and drain-grace controls) drop elapsed-time ceilings that duplicated an outcome assertion already proving the deadline or grace fired (`timed_out`, `pipe_held_after_exit`, the truncation diagnostic), so required runs stop failing on runner speed (#10483).
