<!-- section: Fixed -->
- TypeScript: a `typeof` clause on a line of a multi-line `return`, a conditional arm or a returned object property no longer reads `exposed` when every asserted call takes the same value under the old and the changed expression; the statement-level expression is evaluated on the asserted inputs instead of the continuation line alone (#12380).
