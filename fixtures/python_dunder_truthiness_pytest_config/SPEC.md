# Fixture: python_dunder_truthiness_pytest_config

Spec: RIPR-SPEC-0238

## Given

An isolated integer truthiness edit and pytest collection configuration changes the test naming pattern so the apparent test is not collected.

## When

`cargo xtask fixtures python_dunder_truthiness_pytest_config`

## Then

The finding stays `static_unknown`; the test is not a valid discriminator.

## Must Not

Credit the assertion as exposed under unsupported collection or message behavior.
