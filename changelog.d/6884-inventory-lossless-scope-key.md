<!-- section: Fixed -->
- Seam-inventory scope sets now key on lossless byte identities instead
  of lossy strings, so two files whose names differ only in invalid
  UTF-8 bytes (Unix) or lone surrogates (Windows) no longer merge in
  scope: scoping one leaves its sibling out. Slash normalization and
  `./` stripping are unchanged; display and role consumers keep the
  lossy rendering (#6884).
