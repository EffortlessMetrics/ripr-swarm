# Fixture: rust_where_bound_declarations

Spec: RIPR-SPEC-0001

## Given

A generic function gains a compile-time bound while a real uppercase-named
record field gains a runtime value. The test observes the returned field.

## When

```bash
cargo xtask fixtures rust_where_bound_declarations
```

## Then

The changed `T: Into<u8> + Copy,` bound stays visible as a `static_unknown`
probe, without runtime field-oracle authority or exposure credit. The actual
`Upper: value.into(),` initializer keeps its field-construction probe.

## Must Not

- Read the uppercase identifier or colon as declaration authority without a parser.
- Drop or grant test credit to the changed compile-time bound.
- Erase the uppercase runtime field beside the declaration.
- Change public output schemas, suppressions, thresholds or oracle strength.
