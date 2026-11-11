<!-- section: Fixed -->
- Rust field-write observation no longer credits an owner call when the test
  binding names a different impl type with a same-named method, so a dependency
  type such as `OtherInventory::add` cannot stand in for `Inventory::add`
  (#7683).
