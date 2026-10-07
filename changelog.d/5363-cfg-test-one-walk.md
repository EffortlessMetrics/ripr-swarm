<!-- section: Changed -->
- A warm `ripr check` on a workspace with large test files is faster:
  deciding whether each function sits inside a `#[cfg(test)]` module now
  walks each file once instead of once per function (#5363).
