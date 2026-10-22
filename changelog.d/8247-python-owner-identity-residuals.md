<!-- section: Fixed -->
- Python preview no longer credits `exposed` for an owner call when the asserting
  scope replaces the imported name with a walrus target, a `match` capture or a
  module-level reassignment or `def` after the import, nor when the asserted
  `.method(` runs on a receiver that is not bound to the owner class while a
  different receiver in the same test reaches the owner. Real imports, helper-local
  shadows and bound-receiver assertions keep credit (#8247).
