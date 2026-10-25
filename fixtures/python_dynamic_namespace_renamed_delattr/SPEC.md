# Fixture: python_dynamic_namespace_renamed_delattr

Spec: RIPR-SPEC-0028

## Given

A test uses `mp.delattr(route, "response")` to change a module namespace before calling the
owner and asserting on an imported module object. A separate test uses the same
writer spelling on a named constant before testing a predicate boundary.

## When

The module object's field write changes from 422 to 400 and the predicate
changes from `>` to `>=`.

## Then

Both changed behaviors remain `weakly_exposed`. A namespace writer can replace
or remove the names that the owner reads, so neither the imported object nor
constant supplies stable credit. The constant is also withheld as a repair
target. RIPR-SPEC-0108 pins the two findings independently of golden blessing.

These are static analyzer stimuli, not executed pytest fixtures or a claim
that their assertions pass at runtime. The local `mp = monkeypatch` alias intentionally
avoids the literal `monkeypatch.delattr` limit while retaining the known
auxiliary fixture. A bare `mp` parameter already fails closed through the
unresolved-fixture limit on this base.

## Must Not

Promote either finding, drop either changed behavior, or turn the refusal into
a global mock limit that suppresses independent instance-field observations.
