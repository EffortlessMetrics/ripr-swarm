<!-- section: Fixed -->
- LSP watched-file drift now compares related-test and committed-source paths with the host-aware case identity, so a rewrite or deletion recorded under a differently cased root still schedules a diagnostics refresh on case-insensitive hosts (#14729).
