<!-- section: Internal / CI -->
- Replace the repository-policy timeout test’s naturally expiring descendant with a FIFO-blocked witness; decide termination from liveness, reap the owned tree with bounded cleanup, and reject primary-only signaling controls. (#10593)
