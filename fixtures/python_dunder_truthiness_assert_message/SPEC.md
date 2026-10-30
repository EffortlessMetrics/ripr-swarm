# Fixture: python_dunder_truthiness_assert_message

Spec: RIPR-SPEC-0238

## Given

An isolated integer truthiness edit and an assertion message executes on failure and skips the test instead of failing.

## When

`cargo xtask fixtures python_dunder_truthiness_assert_message`

## Then

The finding stays `static_unknown`; the test is not a valid discriminator.

## Must Not

Credit the assertion as exposed under unsupported collection or message behavior.
