# Fixture: python_dunder_truthiness_cross_file_direct

Spec: RIPR-SPEC-0238

## Given

The exact source and diff retained by the C237 review receipt
`python_dunder_truthiness_integer/evidence/cross-file-direct.receipt.json`.
A separate collected module replaces `Money.__bool__`, so both versions' selected
tests pass. The immutable D491 native report remains preserved alongside that receipt.

## When

`cargo xtask fixtures python_dunder_truthiness_cross_file_direct`

## Then

The changed owner is `static_unknown`: the new truthiness admission sees the
third physical Python source and withholds credit, including when the general
walker omits a generated filename. The same producer keeps the isolated two-file
positive `python_dunder_truthiness_integer` exposed.

## Must Not

Credit a project-mutated class, rewrite the historical observed report, or grant
an absence claim because the general source walker omitted a file.
