# Fixture: python_message_pin_local_no_exposed_credit

Spec: RIPR-SPEC-0238

## Given

`check` changes only the message it assigns to the local `msg` from
`negative value: must be at least zero` to `negative value: must be at most
zero`, and raises it through the one `raise ValueError(msg)`. The only test
asserts the error with `pytest.raises(ValueError, match="negative value")`, a
literal pattern that matches the old and the new message alike. This is the
two-step shape of the attrs `default_if_none` instance in #9185.

## When

`cargo xtask fixtures python_message_pin_local_no_exposed_credit`

## Then

The finding is `weakly_exposed`, not `exposed`: the pattern pins only the
exception class, so the suite passes with the old and the new message. The
missing discriminator names the changed message.

## Must Not

Promote the finding to `exposed` while every strong error assertion uses a
pattern that fits both messages. The governed RIPR-SPEC-0108 honesty corpus
independently forbids promotion.
