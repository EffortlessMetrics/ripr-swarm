# Fixture: python_module_generator_test

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

An unchanged test has the `unreached-generator-assignment` identity or reachability counterexample.

## When

The single changed module constant or conditional return is analyzed.

## Then

One nonempty `static_unknown` finding withholds credit for the changed behavior.
The unchanged Python observation is the same before and after the edit.

## Must Not

Credit a replaced callable, dead branch, or assertion the test never reaches.
The honesty corpus independently rejects promotion after an incorrect re-bless.
Direct generator invocation is a CPython observation, not a pytest execution claim.
