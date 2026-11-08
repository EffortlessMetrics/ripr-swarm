<!-- section: Fixed -->
- A panic while the seam-evidence pass primes its shared workspace tables is now
  contained instead of unwinding the run: every seam of that pass reads as an
  opaque stand-in whose stages name `seam_evidence_incomplete`, with no related
  test, discriminator or repair signal, and the possibly-incomplete memo is
  never read. The panic payload is not kept in findings (#11524).
