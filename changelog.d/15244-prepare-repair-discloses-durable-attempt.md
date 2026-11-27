<!-- section: Fixed -->
- MCP `ripr_prepare_repair` no longer creates a second in-memory attempt beside a
  live durable attempt for the same seam: it discloses the durable attempt
  (`existing_attempt`, links) and routes to `ripr_get_repair_attempt`, matching
  the CLI's single-attempt rule (#15244). Because a rival attempt cannot be
  ruled out otherwise, a ready item now fails closed with a typed error when
  its seam binding is missing (`no_snapshot`, `seam_not_found`), when the HEAD,
  attempt store or any stored manifest cannot be read (`analysis_failed`), or
  when a refresh replaced the snapshot during the store read (`stale_snapshot`).
