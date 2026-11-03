# Fixture: python_bool_owner_mock_result_escape

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A boolean owner changes its boundary comparison from `>` to `>=`.
A test assigns a genuine mock's returned child to another name, configures
its side effect to change the owner's default threshold, then invokes it.

## When

The changed comparison is analyzed.

## Then

One `weakly_exposed` finding with a `smoke_only` / `smoke` oracle. Direct
construction does not authenticate uses of an escaped call result.

## Must Not

Promote the bare assertion to an exact pin. Native Python passes even with
the wrong `>` boundary because the callback changes the default threshold.
The honesty corpus independently pins both classification and weak oracle.
