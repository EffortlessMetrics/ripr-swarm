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
  The same gate now also guards the per-attempt `repair_attempts[].command`
  in status JSON and the start `ripr pilot` recorded, withholds a start it
  cannot verify (for example a seam id that moved) instead of printing it, and
  explains an already-gripped seam as a static reading rather than telling the
  reader to strengthen a test.
  Recorded refusal text, receipt recommendations, the diverged-head restart and
  the after-phase narration that `ripr continue` prints drop the new-attempt
  recipe for such a seam, while the reset-and-rerun-after recovery stays. The
  final error `ripr agent repair --phase after` gives for an attempt that
  already ended is not covered.
