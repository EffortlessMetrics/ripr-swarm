<!-- section: Fixed -->
- Rust finding IDs now hash normalized discriminators longer than 256 bytes, so a long
  string literal fits in the same repair, explain and receipt commands as a short one.
  Refresh affected baseline entries and use the new IDs for future receipts (#15353).
