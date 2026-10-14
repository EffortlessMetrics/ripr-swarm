# ripr golden drift report

Status: warn

This report summarizes expected-output drift for reviewer inspection. It never blesses goldens.

## Provenance

- binary: `/workspace/scratch/2e70149a8e59/repair-private-target-7415-target/debug/ripr`
- ripr version: `ripr 0.11.0 (6b382125783a05e9b3353131837ed215fce70247)`
- revision: `6b382125783a05e9b3353131837ed215fce70247`

## Summary

- drift entries: 3
- fixture errors: 0

## Drift

### `owner_return_pin_fallible_literal` `check.json`

- expected: `fixtures/owner_return_pin_fallible_literal/expected/check.json`
- actual: `target/ripr/fixtures/owner_return_pin_fallible_literal/check.json`
- changed lines: 23
- changed exposure classes:
  - `expected [exposed] -> actual [weakly_exposed]`
- changed recommended next steps:
  - `expected [] -> actual [The related test `observes_answer` already asserts an exact value; the gap is not the assertion shape. Add a test input where the changed branch behaves differently and keep the exact assertion.]`
- static-language boundary: pass
- blessing reason required: yes
- blessing reason present in PR: no

### `owner_return_pin_fallible_literal` `human.txt`

- expected: `fixtures/owner_return_pin_fallible_literal/expected/human.txt`
- actual: `target/ripr/fixtures/owner_return_pin_fallible_literal/human.txt`
- changed lines: 25
- static-language boundary: pass
- blessing reason required: yes
- blessing reason present in PR: no

### `owner_return_pin_fallible_literal` `human-full.txt`

- expected: `fixtures/owner_return_pin_fallible_literal/expected/human-full.txt`
- actual: `target/ripr/fixtures/owner_return_pin_fallible_literal/human-full.txt`
- changed lines: 14
- static-language boundary: pass
- blessing reason required: yes
- blessing reason present in PR: no

## Fixture / Golden Violations

## Violations

```text
check.json drift for fixture `owner_return_pin_fallible_literal`
  expected: fixtures/owner_return_pin_fallible_literal/expected/check.json
  actual:   target/ripr/fixtures/owner_return_pin_fallible_literal/check.json
  diff:    line 6 expected `  \"summary\": {\"changed_rust_files\":2,\"probes\":1,\"findings\":1,\"exposed\":1,\"weakly_exposed\":0,\"reachable_unre…` vs actual `  \"summary\": {\"changed_rust_files\":2,\"probes\":1,\"findings\":1,\"exposed\":0,\"weakly_exposed\":1,\"reachable_unre…`
```

```text
human.txt drift for fixture `owner_return_pin_fallible_literal`
  expected: fixtures/owner_return_pin_fallible_literal/expected/human.txt
  actual:   target/ripr/fixtures/owner_return_pin_fallible_literal/human.txt
  diff:    line 5 expected `Summary: 1 probe(s), 1 exposed, 0 weakly_exposed, 0 reachable_unrevealed, 0 no_static_path, 0 infection_unknown, 0 propa…` vs actual `Summary: 1 probe(s), 0 exposed, 1 weakly_exposed, 0 reachable_unrevealed, 0 no_static_path, 0 infection_unknown, 0 propa…`
```

```text
human-full.txt drift for fixture `owner_return_pin_fallible_literal`
  expected: fixtures/owner_return_pin_fallible_literal/expected/human-full.txt
  actual:   target/ripr/fixtures/owner_return_pin_fallible_literal/human-full.txt
  diff:    line 5 expected `Summary: 1 probe(s), 1 exposed, 0 weakly_exposed, 0 reachable_unrevealed, 0 no_static_path, 0 infection_unknown, 0 propa…` vs actual `Summary: 1 probe(s), 0 exposed, 1 weakly_exposed, 0 reachable_unrevealed, 0 no_static_path, 0 infection_unknown, 0 propa…`
```

