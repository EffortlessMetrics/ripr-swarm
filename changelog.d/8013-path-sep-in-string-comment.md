<!-- section: Internal / CI -->
- Unit tests pin that a `::` inside a string or comment no longer suppresses a
  real `field_construction` probe (`Foo { path: "a::b" }`, `x: 1, // see Foo::bar`)
  (#8013).
