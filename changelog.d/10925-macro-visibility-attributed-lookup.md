<!-- section: Changed -->
- `ripr check` no longer parses every Rust file in the workspace to learn which module names carry a `#[path]` or other attribute when it resolves the macros a test file inherits from its parent module: the lookup is answered per module name and parses only files whose text can spell `mod <name>` with an attribute. Findings and JSON output are unchanged (#10925).
