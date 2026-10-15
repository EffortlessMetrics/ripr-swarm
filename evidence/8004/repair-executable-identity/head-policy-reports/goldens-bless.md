# ripr goldens bless report

Status: pass

Fixture:
- `fixtures/inherent_receiver_helper_scope_shadow`

Reason:
```text
RIPR-SPEC-0005 and RIPR-SPEC-0108: initialize native-verified function declaration shadows of a helper module and nominal receiver type; the genuine module helper remains admitted. No type arguments or return/value flow are resolved. Existing expectations and runtime labels are unchanged.
```

Actual outputs:
- `target/ripr/fixtures/inherent_receiver_helper_scope_shadow/check.json`
- `target/ripr/fixtures/inherent_receiver_helper_scope_shadow/human.txt`

Updated:
- `fixtures/inherent_receiver_helper_scope_shadow/expected/check.json`
- `fixtures/inherent_receiver_helper_scope_shadow/expected/human.txt`
- `fixtures/inherent_receiver_helper_scope_shadow/expected/CHANGELOG.md`
