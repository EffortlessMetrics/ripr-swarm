<!-- section: Changed -->
- `ripr first-pr --check` now exits 2 when the current start-here packet records a blocked state (wrong root, missing or malformed artifacts), and `ripr help first-pr` documents the exit contract; write mode still exits 0 after composing the packet (#7373).
