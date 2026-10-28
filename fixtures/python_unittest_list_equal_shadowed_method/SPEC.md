# Fixture: python_unittest_list_equal_shadowed_method

Spec: RIPR-SPEC-0028

## Given

A list return changes from `[1, 1]` to `[1, 2]`. An earlier unittest
method asserts the new result, but a later method with the same name replaces
it in the class. Default unittest discovery runs only the later method.

## When

`cargo xtask fixtures python_unittest_list_equal_shadowed_method`

## Then

The shadowed assertion must not promote the finding to exposed or claim a
strong exact oracle. The RIPR-SPEC-0108 corpus independently forbids promotion.

## Must Not

Report `exposed`, a strong exact oracle, or discrimination yes for the
shadowed assertion.
