<!-- section: Fixed -->
- After a failed editor refresh, the server drops the uncovered save digests before it logs the failure, so a client that re-saves identical bytes in response to that log is no longer deduplicated and gets a retry (#10033, #10038).
