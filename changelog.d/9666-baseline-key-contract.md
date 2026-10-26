<!-- section: Internal / CI -->
- Baseline fallback identity: the three duplicated `path:line:static_class` builders now share one documented stored-string owner (host-independent `\` to `/`, no case folding) and are deliberately not migrated to the host-dependent `path_identity_key`, so existing baselines keep matching (#9666).
