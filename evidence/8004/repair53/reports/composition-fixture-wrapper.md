# ripr fixtures report

Status: pass

Requested fixture: `inherent_receiver_local_wrapper`

## Fixtures

- `fixtures/inherent_receiver_local_wrapper`

## Actual Outputs

- `inherent_receiver_local_wrapper` -> `target/ripr/fixtures/inherent_receiver_local_wrapper`
  - `target/ripr/fixtures/inherent_receiver_local_wrapper/check.json`
  - `target/ripr/fixtures/inherent_receiver_local_wrapper/human.txt`

## Golden Comparisons

- `inherent_receiver_local_wrapper` `check.json`: pass
  - expected: `fixtures/inherent_receiver_local_wrapper/expected/check.json`
  - actual: `target/ripr/fixtures/inherent_receiver_local_wrapper/check.json`
- `inherent_receiver_local_wrapper` `human.txt`: pass
  - expected: `fixtures/inherent_receiver_local_wrapper/expected/human.txt`
  - actual: `target/ripr/fixtures/inherent_receiver_local_wrapper/human.txt`
- `inherent_receiver_local_wrapper` `human-full.txt`: pass
  - expected: `fixtures/inherent_receiver_local_wrapper/expected/human-full.txt`
  - actual: `target/ripr/fixtures/inherent_receiver_local_wrapper/human-full.txt`

## Violations

None detected.
