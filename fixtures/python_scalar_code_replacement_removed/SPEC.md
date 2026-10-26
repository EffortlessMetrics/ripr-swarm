# Fixture: python_scalar_code_replacement_removed

Spec: RIPR-SPEC-0028

## Given

The reader of `python_scalar_code_replacement` with the code replacement removed.

## When

The constant changes from 5 to 6.

## Then

The finding stays `exposed`: the reader is unchanged, so the assertion fails after the edit.

## Must Not

Refuse every scalar read. This control keeps the refusal tied to the code replacement.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native Python runs the unchanged test on the original and edited module: the trap passes both (0 of 1 versions fail), the control fails only after the edit.
