Rust where clauses and generic type/lifetime bounds without expression or macro
descendants now retain parser-confirmed static limitations rather than runtime
field-construction obligations. Const expressions and macro types retain existing
fallback evidence. Removed
bounds use reconstructed old-side syntax; genuine record fields keep their
runtime classification. No test-exposure credit or output schema changes.
