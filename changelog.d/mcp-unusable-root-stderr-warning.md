<!-- section: Fixed -->
- `ripr mcp --stdio` now prints one stderr warning line at startup when the workspace root is unusable (nonexistent, a file, or not a repository root), instead of failing only as `workspace_unavailable` in tool output; stdout stays protocol-only and tools still fail closed (#2379, #8716).
