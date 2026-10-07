# Golden Output Changes

## Pending — rust_adversarial_free_function_receiver (1)

Reason:
RIPR-SPEC-0108: new false-exposed guard for #7006; receiver-qualified-only call against a free-function owner reads weakly_exposed via weak_token_substring, never exposed

Command:
`cargo xtask goldens bless rust_adversarial_free_function_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_adversarial_free_function_receiver (2)

Reason:
RIPR-SPEC-0108: register expected/human-full.txt for the #7006 false-exposed guard so the honesty corpus can pin its class

Command:
`cargo xtask goldens bless rust_adversarial_free_function_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
