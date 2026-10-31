<!-- section: Added -->
- `ripr repair --base REF` names the diff base that scopes an automatic start,
  as on `ripr check`, and wins over the inferred base. When repair cannot read
  which lines changed (no base found, `REF` unreadable, or nothing differs
  from the base), its refusal now says so first instead of listing code as if
  it were scoped to your change (#11246).
