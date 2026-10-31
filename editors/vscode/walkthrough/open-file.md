# Open a Rust file
Open any Rust file in your workspace. ripr starts the server, resolves the
matching binary if needed, and analyzes the saved workspace in the background.
This step completes when the active editor shows a Rust file.

The editor also routes TypeScript/JavaScript and Python files to ripr, but the
server analyzes a preview language when it is auto-detected (below) or after you
enable it. To enable it explicitly, create `ripr.toml` at the workspace root with

```toml
[languages]
enabled = ["rust", "typescript"]
```

then run **ripr: Restart Server** (or reload the window) so the server picks up
the new language set. With no `ripr.toml`, Python auto-enables on Python project
markers or detectable `.py` files under root `src/` or `tests/`, and
TypeScript auto-enables on a `tsconfig.json` or `jsconfig.json`, or
detectable `.ts`/`.tsx` files under `src/` or `tests/`; a `package.json` or
JavaScript files alone do not enable it. Per-language conditions:
[Support tiers](https://github.com/EffortlessMetrics/ripr/blob/main/docs/status/SUPPORT_TIERS.md).
