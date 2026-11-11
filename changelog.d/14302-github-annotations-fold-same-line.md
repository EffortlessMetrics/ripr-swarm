<!-- section: Changed -->
- `ripr check --format github`: two probes of one class on one changed line (for example the predicate and return-value probes of `a.len() == 0`) now render one annotation that says it covers the other, matching SARIF's one alert per line, instead of two near-identical warnings (#14302).
