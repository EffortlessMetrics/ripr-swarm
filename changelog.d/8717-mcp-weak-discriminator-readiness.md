<!-- section: Fixed -->
- MCP repair readiness no longer refuses a `weakly_exposed` finding merely
  because it names its missing discriminator: that named value is the repair
  target, so the finding-level discriminator gate passes and the fix-site gate
  and the shared `ripr repair` seam validator still decide. A named missing
  discriminator on any other class, a weak finding without a populated
  canonical gap, and a weak finding without a fix site stay refused with their
  typed reasons (#8717).
