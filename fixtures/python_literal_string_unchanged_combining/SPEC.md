# Fixture: python_literal_string_unchanged_combining

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

A literal string changes its first character while the test indexes an unchanged combining character.

## When

The return edit is analyzed.

## Then

One `weakly_exposed` finding withholds credit. Both Python versions pass the test.

## Must Not

Promote a test that only reads the unchanged indexed character.
