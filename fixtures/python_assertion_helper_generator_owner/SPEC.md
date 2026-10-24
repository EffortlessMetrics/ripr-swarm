# Fixture: python_assertion_helper_generator_owner

Spec: RIPR-SPEC-0238

## Given

A same-file helper reads the unchanged counter after a generator method's returned object is closed without executing the method body.

## When

The expanded counter update changes to `+=`.

## Then

The finding remains `weakly_exposed`. Both the original and deleted-write test bodies pass, so creating the deferred object supplies no observed write.

## Must Not

Promote a helper comparison through the syntactic call alone. The generator control places `yield` inside an f-string replacement expression.
The RIPR-SPEC-0108 honesty corpus independently checks the rendered class.
