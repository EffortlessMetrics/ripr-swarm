# Fixture: python_message_pin_local_control

Spec: RIPR-SPEC-0238

## Given

The same two-step edit as `python_message_pin_local_no_exposed_credit`
(`msg = "..."` then `raise ValueError(msg)`, `at least` to `at most`), but the
test asserts `pytest.raises(ValueError, match="at most zero")`. That pattern
accepts the new message and rejects the old one.

## When

`cargo xtask fixtures python_message_pin_local_control`

## Then

The finding stays `exposed`: the pattern tells the old and the new message
apart, so the two-step message gate does not engage. This is the positive
control for the two-step reading.

## Must Not

Demote the finding to `weakly_exposed` when a pattern discriminates the message.
