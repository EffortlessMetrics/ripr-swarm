# Fixture: unsafe_boundary_probe

Spec: RIPR-SPEC-0168

## Given

The #3536 unsafe-boundary shape: the diff changes the loop body strictly
inside an explicit `unsafe {}` block in `sum_bytes`, with a second single-line
unsafe block (`read_first`) and production code outside any boundary as
controls.

## When

```bash
cargo xtask fixtures unsafe_boundary_probe
```

## Then

The changed line inside the boundary projects exactly one parser-backed
`unsafe_boundary` probe with the `static_unknown` family at the changed line,
deduplicated by the boundary's byte identity, while ordinary probes for the
surrounding function are preserved and no unsafe-boundary probe attaches to
`read_first` or to code outside a boundary.

## Must Not

- Promote reach plus an oracle to `exposed` for the boundary probe; the
  `static_unknown` family short-circuits before any reach or oracle logic.
- Attach an unsafe-boundary probe to lines outside a boundary, to
  `read_first`, or to a boundary edge line shared with outside code.
- Change the output schema; the probe id and family strings are the already
  registered `static_unknown` values.
