<!-- section: Fixed -->
- TypeScript no longer treats `const r = helper.price(150)` as an observation of a bare-call owner `price`. The one-hop alias initializer now uses the same bare-reference rule as the #7507 value guard; method owners still credit `receiver.method(...)` (#7813).
