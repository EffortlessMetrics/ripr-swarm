<!-- section: Fixed -->
- A changed Python `return {...}` dict now names the changed entry as the
  missing check, not the first entry. Before, a renamed key (`"quantity"` to
  `"qty"`) whose only test asserted the unchanged `"sku"` key was told to add
  `sku == <expected value>`, the check that test already makes; it now names
  `qty`. When several entries changed, or an entry was removed beside a changed
  one, the missing check names the whole returned dict instead of guessing one
  key. Verdicts are unchanged (#13630).
