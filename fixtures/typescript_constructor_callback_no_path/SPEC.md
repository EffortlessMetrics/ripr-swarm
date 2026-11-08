# Fixture: typescript_constructor_callback_no_path

Spec: RIPR-SPEC-0243

## Given

An exported const whose initializer is a call with a function-valued argument
(`export const ZodTransform = constructor('ZodTransform', (inst, def) => { ... })`).
The changed line is a statement inside the callback's per-call parse function.
No test references `ZodTransform`.

## When

```bash
cargo xtask fixtures typescript_constructor_callback_no_path
```

## Then

The finding stays `no_static_path`, and its missing-check and next-step wording
says the changed code runs when a value built from `ZodTransform` is used, not
at module load, instead of asking for an `expect(IMPORTED_CONST)` observer.
The negative control is `typescript_module_value_unused`, a plain constant
initializer that keeps the module-initializer wording.

## Must Not

- Offer the module-initializer constant-observer advice for a per-call callback
  statement (#13588).
- Promote the finding or make it repair-ready.
