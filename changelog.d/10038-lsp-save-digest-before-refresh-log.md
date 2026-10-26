<!-- section: Fixed -->
- Re-saving identical bytes in response to an editor refresh-failure log no longer races save deduplication. The server now drops uncovered save digests before sending that log, allowing the save to request a retry (#10033, #10038).
