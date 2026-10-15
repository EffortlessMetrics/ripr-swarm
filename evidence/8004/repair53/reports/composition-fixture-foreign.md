# ripr fixtures report

Status: pass

Requested fixture: `inherent_receiver_foreign_helper`

## Fixtures

- `fixtures/inherent_receiver_foreign_helper`

## Actual Outputs

- `inherent_receiver_foreign_helper` -> `target/ripr/fixtures/inherent_receiver_foreign_helper`
  - `target/ripr/fixtures/inherent_receiver_foreign_helper/check.json`
  - `target/ripr/fixtures/inherent_receiver_foreign_helper/human.txt`

## Golden Comparisons

- `inherent_receiver_foreign_helper` `check.json`: pass
  - expected: `fixtures/inherent_receiver_foreign_helper/expected/check.json`
  - actual: `target/ripr/fixtures/inherent_receiver_foreign_helper/check.json`
- `inherent_receiver_foreign_helper` `human.txt`: pass
  - expected: `fixtures/inherent_receiver_foreign_helper/expected/human.txt`
  - actual: `target/ripr/fixtures/inherent_receiver_foreign_helper/human.txt`
- `inherent_receiver_foreign_helper` `human-full.txt`: pass
  - expected: `fixtures/inherent_receiver_foreign_helper/expected/human-full.txt`
  - actual: `target/ripr/fixtures/inherent_receiver_foreign_helper/human-full.txt`

## Violations

None detected.
