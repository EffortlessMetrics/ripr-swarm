<!-- section: Fixed -->
- TypeScript: a literal message check now counts as an exact error oracle.
  chai `expect(fn).to.throw("message")`, and `node:assert` `throws` or
  `rejects` with an anchored regex (`/^Error: message$/`) or a
  `{ message: "..." }` object, read `exact_error_variant` / strong instead
  of `broad_error` / weak, so a test that pins the thrown message no longer
  leaves an actionable `weakly_exposed` gap. When only the message text
  changed, the check credits only if it passes on exactly one of the old
  and new messages: a chai string inside both messages, or a regex that is
  not a plain literal, still reads `broad_error`. A regex with flags stays
  `broad_error` (RIPR-SPEC-0243 rule 10, #6654).
