<!-- section: Fixed -->
- A named field declared inside an enum variant (`InvalidLastSymbol {
  offset: usize, .. }`, one line or one field per line) is now type syntax,
  not a `field_construction` probe. Like struct field declarations, it stays
  one visible `static_unknown` probe; a variant constructed in a body still
  seeds `field_construction` (#8999).
