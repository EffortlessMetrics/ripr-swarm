<!-- section: Added -->
- The TypeScript verdict corpus gains nine runtime-labeled cases for the
  acceptance examples that had none: strict identity matchers and the
  dropped-field forms (RIPR-SPEC-0246 examples 11 to 14), a `jest.spyOn` on an
  exported module object (RIPR-SPEC-0247 example 12), an assertion-shaped
  call on a helper receiver (RIPR-SPEC-0252 example 6) and the refused custom
  matcher forms (RIPR-SPEC-0262 examples 2 to 4). All nine score ideal, so
  spec-example coverage rises from 112 to 121 of 121 and the ledger floor
  with it (#6654).
