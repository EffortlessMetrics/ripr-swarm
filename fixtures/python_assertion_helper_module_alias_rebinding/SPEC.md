# Fixture: python_assertion_helper_module_alias_rebinding

Spec: RIPR-SPEC-0238

## Given

The imported module alias is replaced with an object that constructs a stand-in. Calling the same method spelling on that stand-in must not credit the production field write.

## When

The expanded counter update changes to `+=`.

## Then

One `Stats.record_write` finding remains `weakly_exposed`.

## Must Not

Promote a helper comparison without the executed receiver and retained field identity. Deleting the changed write leaves this control satisfied. RIPR-SPEC-0108 independently checks the class and nonempty finding count.
