<!-- section: Fixed -->
- TypeScript verdicts follow RIPR-SPEC-0234 rules 3 to 8 and 10. chai
  `to.throw("msg")` and `node:assert` `throws`/`rejects` with an anchored
  regex or a literal `message` object now pin the thrown error; an error
  payload that matches both the old and new message, or a bare `Error`
  class, no longer credits a message change; a file-local or chai `expect`
  is not read as Jest's; a default import credits only a default-exported
  owner; owner and changed-token matches use whole identifiers (`address`
  no longer observes `add`); an `import { f as p }` local counts as the
  owner; a changed line whose imported callee every crediting test file
  mocks reads `static_unknown`. Inline literal `test.each` / `it.each`
  tables are read one concrete case per row. On the TypeScript verdict
  corpus false verdicts drop from 36/101 to 21/101: false actionable 27/62
  to 19/62, false exposed 9/39 to 2/39 (#6654).
