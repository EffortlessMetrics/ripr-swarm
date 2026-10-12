<!-- section: Fixed -->
- A direct collection assertion such as `items.len() * 0`, `items.is_empty() || true`,
  or `items.capacity() >= 1` no longer reads `exposed` for an `items.push`
  mutation. The subject must be exactly the collection, an index/slice of it,
  or one value-read that ends there; `capacity` is not a value-read. Ordinary
  `len()`, index, and whole-collection equalities still get credit (#7135).
