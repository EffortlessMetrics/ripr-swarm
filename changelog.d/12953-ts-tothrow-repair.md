<!-- section: Fixed -->
- A TypeScript weakly-exposed thrown-variant change whose test uses a bare
  `toThrow()` now projects a validator-complete repair packet through the
  shared `repair_packet_ready` validator, with the edit target pinning the
  named variant (for example `toThrow(TooLongError)`), instead of refusing
  `ripr agent repair` with "projects no complete TypeScript repair record".
  A discriminator that names no specific variant (such as the generic
  `Error` class) still projects no packet and stays not-ready (#12953).
