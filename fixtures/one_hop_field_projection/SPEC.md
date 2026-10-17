# Fixture: one_hop_field_projection

Spec: RIPR-SPEC-0159

## Given

A changed primitive count field and the retained projected_positive_two_fields assertion control.

## When

`cargo xtask fixtures one_hop_field_projection` analyzes the single-line field change.

## Then

The field reports `exposed`. The exact projected assertion supplies the observation.

## Must Not

Pool reach and unrelated equality, credit a discarded result, or substitute a sibling field.
