<!-- section: Fixed -->
- A TypeScript changed line inside a lone `case <literal>:` clause of `switch (identifier)` now reads `weakly_exposed` when every related test call provably selects another clause, instead of `exposed`. Stacked labels, `default`, fall-through clauses, computed discriminants and non-literal labels keep the existing verdict (#8569).
