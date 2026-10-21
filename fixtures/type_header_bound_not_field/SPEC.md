# Fixture: type_header_bound_not_field

Spec: RIPR-SPEC-0001

## Given

A new `enum` declaration carries a generic bound (`Q: Ord + Copy`) on its
header line. The bound only looks like a field `name: Type`.

## When

The enum is added and the public analyzer reads the fixture.

## Then

The header is one `static_unknown` probe, so a changed bound is disclosed. No
`field_construction` probe, and no field-value assertion request, is attached
to it (#9170).

## Must Not

- Report a `field_construction` probe or a `weakly_exposed` /
  `no_static_path` warning for a type declaration header.
- Drop the header silently.
