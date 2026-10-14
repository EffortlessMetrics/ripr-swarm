<!-- section: Fixed -->
- A Rust test that compares an owner's struct with `expected()`, a helper
  that only returns a struct literal, or with `T { .., ..T::default() }`
  over a manual `Default` that only returns a struct literal, now credits
  the changed field the literal pins instead of reporting a missing
  discriminator. A once-bound `let c = owner(..).await.unwrap();` of an
  async owner also credits now, as the inline form does (RIPR-SPEC-0225
  decision 7, #6692).
