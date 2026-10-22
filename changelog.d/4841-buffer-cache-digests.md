<!-- section: Changed -->
- Cache integrity checks batch JSON fragments into bounded SHA-256 writes, reducing
  per-token hashing overhead while preserving cache bytes and validation (#4841).
