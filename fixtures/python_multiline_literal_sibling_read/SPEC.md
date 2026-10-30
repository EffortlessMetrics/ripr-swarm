# Fixture: python_multiline_literal_sibling_read

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

A multi-line dict literal renames its `tag` key to `label` on a line of its own.
The only test subscripts the unchanged `name` key and passes before and after the edit.

## When

The single changed element line is analyzed.

## Then

One `weakly_exposed` finding withholds credit: the test reads a sibling, not the changed key.

## Must Not

Credit an assertion that reads only an unchanged sibling of a multi-line literal element.
The honesty corpus independently rejects promotion after an incorrect re-bless.
