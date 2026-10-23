<!-- section: Fixed -->

- `ripr init --help` no longer says `--force` overwrites an existing
  `ripr.toml` unconditionally: it now states that plain `ripr init --force`
  resets the config, while with `--ci` or `--mcp` the config is always kept and
  `--force` only replaces the generated workflow or the `ripr` MCP entry (#9922).
