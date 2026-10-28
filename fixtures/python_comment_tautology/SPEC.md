# Fixture: python_comment_tautology

Spec: RIPR-SPEC-0238

## Given

A changed arithmetic return and a test whose assertion does not discriminate
that return. An inert comment sits inside the assertion's expression span.

## When

`cargo xtask fixtures python_comment_tautology` analyzes the retained diff.

## Then

Exactly one return-value finding names `fee` and remains `weakly_exposed`.
The actual CLI metamorphic test checks the same inputs without the comments
and keeps a genuine return-value assertion as a positive control.

## Must Not

Credit a comment as an owner reference or as a difference between otherwise
identical comparison operands. Quoted hash characters remain literal data.
