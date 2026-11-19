<!-- section: Fixed -->
- A Rust source file whose name contains `%` now gets a repair start from `ripr check`, and `ripr repair` can bind its inline test module. Packet and gap paths spell `%` as `%25`, and both consumers now decode that before opening the file (#15354).
