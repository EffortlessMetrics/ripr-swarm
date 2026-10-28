<!-- section: Fixed -->
- Rust boundary pairing no longer combines the changed owner's far-input
  oracle with a same-named sibling's boundary assertion, including let-bound
  results and imported calls. Block results use their returned call rather than
  discarded statements. Unresolved rival bindings stay unconfirmed (#4828).
