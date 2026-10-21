<!-- section: Docs -->
- `ripr.toml` key retirement now follows a stated policy in `docs/CHANGELOG_POLICY.md`: a deprecation warning for at least one minor release first, then the config load error names the release that retired the key and its replacement instead of a bare `unknown field`. No key is retired yet (#7453).
