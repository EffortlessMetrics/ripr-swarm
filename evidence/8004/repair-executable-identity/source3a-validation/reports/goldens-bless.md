# ripr goldens bless report

Status: pass

Fixture:
- `fixtures/inherent_receiver_free_helper_identity`

Reason:
```text
RIPR-SPEC-0005 and RIPR-SPEC-0108: initialize the native foreign helper binding refusal with paired real helper controls. Existing goldens and runtime labels are unchanged.
```

Actual outputs:
- `target/ripr/fixtures/inherent_receiver_free_helper_identity/check.json`
- `target/ripr/fixtures/inherent_receiver_free_helper_identity/human.txt`

Updated:
- `fixtures/inherent_receiver_free_helper_identity/expected/check.json`
- `fixtures/inherent_receiver_free_helper_identity/expected/human.txt`
- `fixtures/inherent_receiver_free_helper_identity/expected/CHANGELOG.md`
