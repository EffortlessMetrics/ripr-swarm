Rust where clauses and generic type/lifetime bounds without expression or macro
descendants now retain parser-confirmed static limitations rather than runtime
field-construction obligations. Declaration-only inline struct, impl and
type-alias bounds use their enclosing parser-owned item; shared runtime bodies
retain existing parser/lexical handling. Const expressions and macro types retain existing
fallback evidence. Removed bounds use reconstructed old-side syntax and source
local roles; resolved indexed external test context remains excluded. Genuine
record fields keep their runtime classification. No test-exposure credit or
output schema changes.
