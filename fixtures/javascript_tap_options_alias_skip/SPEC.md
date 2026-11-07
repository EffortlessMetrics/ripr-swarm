# Fixture: javascript_tap_options_alias_skip

Spec: RIPR-SPEC-0085
Spec: RIPR-SPEC-0108

## Given

Tap named registrations use an options alias that suppresses callback execution.

## When

The stringify predicate is inverted; run `cargo xtask fixtures javascript_tap_options_alias_skip`.

## Then

The related callback remains unknown and must not promote to exposed.

## Must Not

- Credit an unexecuted callback.
- Treat the static result as a native runtime verdict.
