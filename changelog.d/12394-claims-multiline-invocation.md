<!-- section: Fixed -->
- A claims `assert_ok_eq!(..)` or `assert_some_eq!(..)` written over several lines now earns the same owner-return credit as its one-line form, when its closing line holds only the `;` and a comment. A second statement on the closing line, a path-qualified macro or a non-literal expected operand still reads `unknown` (#12394).
