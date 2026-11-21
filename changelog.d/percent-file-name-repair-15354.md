<!-- section: Fixed -->
- A Rust source file whose name contains `%` now gets a repair start from `ripr check`, and `ripr repair` builds its edit-cage rules from the real file name. Gap and packet paths spell `%` as `%25`, and both consumers now decode that before using the path (#15354).
