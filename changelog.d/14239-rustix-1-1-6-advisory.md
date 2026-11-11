<!-- section: Security -->
- The locked `rustix` dependency moves from 1.1.5 to 1.1.6, the patched release for RUSTSEC-2026-0486 (unsound extended-attribute functions on a zero-length buffer). ripr itself calls only the process and event parts of `rustix`, so no ripr behavior changes (#14239).
