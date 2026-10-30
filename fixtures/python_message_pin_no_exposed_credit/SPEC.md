# Fixture: python_message_pin_no_exposed_credit

Spec: RIPR-SPEC-0238

## Given

`check` changes only the message of its `raise ValueError(...)` from
`negative value: must be at least zero` to `negative value: must be at most
zero`. The only test asserts the error with
`pytest.raises(ValueError, match="negative value")`, a literal pattern that
matches the old and the new message alike.

## When

`cargo xtask fixtures python_message_pin_no_exposed_credit`

## Then

The finding is `weakly_exposed`, not `exposed`: the pattern pins only the
exception class, so the suite passes with the old and the new message. The
missing discriminator names the changed message. This is the #9185
false-promotion correction.

## Must Not

Promote the finding to `exposed` while every strong error assertion uses a
pattern that fits both messages. The governed RIPR-SPEC-0108 honesty corpus
independently forbids promotion.
