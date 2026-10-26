<!-- section: Internal / CI -->
- Scanner-state transitions now measure their acceptance examples: RIPR-SPEC-0163 examples are numbered, the string-state accept path and the six fail-closed rejects are runtime-labeled corpus cases, and the scanner accepts the `_` catch-all state arm that an exhaustive `&str` match requires (RIPR-SPEC-0163, #9271).
