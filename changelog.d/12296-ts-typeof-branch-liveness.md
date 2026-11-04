<!-- section: Fixed -->
- TypeScript returned `typeof` conditionals whose two arms print the same value for the asserted input (`String(42)` against `JSON.stringify(42)`) no longer read `exposed`: the evaluator now folds `String(...)` and `JSON.stringify(...)` of literal-bound numbers, booleans and strings, so the unexercised-change check withdraws the credit (#12296).
