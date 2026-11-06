<!-- section: Changed -->
- TypeScript warm analysis verifies per-file fact integrity over the stored body bytes instead of serializing the decoded facts again. The internal fact-cache schema advances to 7; existing facts are rebuilt with the same source, configuration, root and dependency checks (#10925).
