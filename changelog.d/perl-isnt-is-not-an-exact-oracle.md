<!-- section: Fixed -->
- Perl (RIPR-SPEC-0235 rule 17): an `isnt(...)` assertion no longer credits a
  changed owner as exposed; it rejects one value and cannot notice the
  changed behavior, so the finding stays a gap (#8730).
