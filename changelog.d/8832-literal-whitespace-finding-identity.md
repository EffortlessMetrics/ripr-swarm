<!-- section: Fixed -->
- Rust finding IDs preserve literal whitespace so a changed string or character
  value cannot inherit an earlier value's suppression; formatting whitespace
  outside literals remains normalized, and old IDs stay stale (#8832).
