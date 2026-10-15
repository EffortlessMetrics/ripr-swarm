# ripr fixtures report

Status: pass

Requested fixture: `inherent_receiver_macro_discard`

## Fixtures

- `fixtures/inherent_receiver_macro_discard`

## Actual Outputs

- `inherent_receiver_macro_discard` -> `target/ripr/fixtures/inherent_receiver_macro_discard`
  - `target/ripr/fixtures/inherent_receiver_macro_discard/check.json`
  - `target/ripr/fixtures/inherent_receiver_macro_discard/human.txt`

## Golden Comparisons

- `inherent_receiver_macro_discard` `check.json`: pass
  - expected: `fixtures/inherent_receiver_macro_discard/expected/check.json`
  - actual: `target/ripr/fixtures/inherent_receiver_macro_discard/check.json`
- `inherent_receiver_macro_discard` `human.txt`: pass
  - expected: `fixtures/inherent_receiver_macro_discard/expected/human.txt`
  - actual: `target/ripr/fixtures/inherent_receiver_macro_discard/human.txt`
- `inherent_receiver_macro_discard` `human-full.txt`: pass
  - expected: `fixtures/inherent_receiver_macro_discard/expected/human-full.txt`
  - actual: `target/ripr/fixtures/inherent_receiver_macro_discard/human-full.txt`

## Violations

None detected.
