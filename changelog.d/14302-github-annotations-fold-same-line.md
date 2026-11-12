<!-- section: Changed -->
- `ripr check --format github`: two probes of one class on one changed line (for example the predicate and return-value probes of `a.len() == 0`) now render one annotation that says it covers the other, like SARIF's one alert per line (a finding with its own repair card or preview actionability keeps its own annotation), instead of two near-identical warnings (#14302).
