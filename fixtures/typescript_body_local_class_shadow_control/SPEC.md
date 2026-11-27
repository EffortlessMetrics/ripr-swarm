# Fixture: typescript_body_local_class_shadow_control

Spec: RIPR-SPEC-0095

Owner: analysis-fixtures

Issue: #14083

## Given

The same changed `Url` constructor as `typescript_body_local_class_shadow`.
The one test imports `Url` from `../src/url` and constructs it directly, with
no body-local class.

## When

`cargo xtask fixtures typescript_body_local_class_shadow_control` runs the public diff analyzer.

## Then

The test is related to the owner by a direct owner call, so the shadow rule
withholds credit only when the body declares a class of the imported name. The
finding reads `weakly_exposed`.

```expected-classes
probe:src_url.ts:typescript_preview:c3882ec3 -> weakly_exposed
```

## Must Not

- Lose the direct owner call when no body-local class shadows the import.
- Claim a runtime mutation outcome; this is static evidence only.
