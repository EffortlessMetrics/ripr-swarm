<!-- section: Fixed -->
- MCP `ripr_prepare_repair` no longer creates a second in-memory attempt beside a
  live durable attempt for the same seam: it discloses the durable attempt
  (`existing_attempt`, links) and routes to `ripr_get_repair_attempt`, matching
  the CLI's single-attempt rule (#15244).
