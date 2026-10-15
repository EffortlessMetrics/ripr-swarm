<!-- section: Fixed -->
- TypeScript: a literal `toHaveBeenCalledWith` payload now credits a changed mock call that is the branch of a braceless `if (...)` or `else` sitting directly in an owner body that holds a nested function elsewhere. Nested, labelled and `else if` branches, property-named heads and an `else` after an automatic semicolon still refuse (RIPR-SPEC-0256, #8050).
