<!-- section: Fixed -->
- `ripr status` and `ripr continue` no longer print a repair start
  (`ripr agent repair ... --phase before`, "start a new attempt", or
  `ripr repair`) that the same build refuses when the repair route is not
  ready, for example after a weak-but-covering test leaves the producer-owned
  discriminator fact missing. They consult the shared repair-admission gate
  and instead say the remaining work is strengthening the oracle of the named
  related test by hand. `ripr repair <seam>` on an already-exposed seam now
  says the existing tests already grip it, and a `ripr pilot` run that ranked
  nothing no longer prints "Run after adding the focused test" (#8766).
