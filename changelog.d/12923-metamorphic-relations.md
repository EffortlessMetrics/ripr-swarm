<!-- section: Internal / CI -->
- Added a generated metamorphic relation suite (`crates/ripr/tests/metamorphic_relations.rs`) running the library `check` entry over Rust, Python and TypeScript boundary and error-path seeds, asserting comment/whitespace, local-rename, unrelated-test, test-order and oracle-strength relations on exposure class and finding identity (#12923).
