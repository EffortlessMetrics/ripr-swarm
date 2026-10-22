<!-- section: Fixed -->

- TypeScript: an edit to a fallback operand of a returned `??` chain no longer reads `exposed` when no related test passes an input that reaches the changed operand; it reads `weakly_exposed` instead (RIPR-SPEC-0246 rule 5, #9437).
