<!-- section: Fixed -->
- `ripr repair`: when the repair adds one new test function inside an inline `#[cfg(test)]` module, the brief labels the anchor "Nearby test (left unchanged)" instead of "Test to extend", which contradicted the instruction to leave existing tests unchanged. Integration test files keep "Test to extend" (#15323).
