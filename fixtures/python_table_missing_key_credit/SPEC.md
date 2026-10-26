# Fixture: python_table_missing_key_credit

Spec: RIPR-SPEC-0028

## Given

The same reader as `python_table_bool_key_get_fallback`, called with a genuinely missing integer key.

## When

The fallback entry for key 2 changes from 5 to 6.

## Then

The finding stays `exposed`: key 3 is absent, so the assertion reads the changed fallback and fails after the edit.

## Must Not

Withhold credit from a table edit that the test provably reads. This control keeps the bool-key refusal from becoming a blanket refusal.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native pytest on the unchanged test passes on the original module and fails on the edited one, so the credit is a genuine kill.
