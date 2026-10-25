# Fixture: typescript_parameter_default_setup_hook

Spec: RIPR-SPEC-0243

## Given

A synchronous Mocha suite callback replaces the real `beforeEach` in a
parameter default. Its body declares an unrelated empty-object `globalThis`
binding, then offers a receiver-construction callback to the replaced hook.
The assertion reads a Decoy receiver rather than `Cart.total`.

## When

`ripr check` analyzes `Cart.total` changing from `return 1` to `return 2`.

## Then

The finding remains `no_static_path`. Parameter-default effects resolve before
body declarations become visible. The independent honesty-corpus row forbids
promotion of this inert receiver setup. The unit matrix also retains the
unshadowed-default refusal and the body-shadow-only `exposed` control.

## Must Not

- Shield a real-global parameter write with a body-local object fact.
- Credit a constructor callback that the runner never registers.
- Grant parameter object identity by evaluating its default value.
- Discard the supported literal-body-object or imported-hook controls.
