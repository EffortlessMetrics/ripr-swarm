<!-- section: Fixed -->
- Agent loop: `ripr explain --format agent` no longer prints `ripr progress:` lines on a piped stderr (same default as `ripr check --format agent`), and a Python or TypeScript preview finding that already reads exposed now ends with `done=` instead of a `stop=` that suggested more test work (#13803).
