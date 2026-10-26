# Spec index fragments

Rows for specs added after the fragment convention. Each file is named
`RIPR-SPEC-NNNN.md` and holds that spec's single index row, linked as
`../specs/<spec file>`, in the same three columns as the table in
[the specs README](../specs/README.md). Fragments exist so two PRs that each add a spec
do not conflict on the end of that table; `cargo xtask check-spec-numbering`
and `check-doc-index` read both places. See [the spec format](../SPEC_FORMAT.md).
