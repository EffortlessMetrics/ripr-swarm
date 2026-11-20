<!-- section: Fixed -->
- `ripr check` now reads a changed computed return (`base * 2` under
  `if express`) as noticed when a test pins the exact value a call returns
  through that branch (`assert_eq!(shipping_cost(100, 10, true), 800)`),
  matching what `ripr repair` already read. A pin from the other branch, an
  inequality, or any owner shape the static fold cannot read keeps the
  earlier weaker reading (#13037).
