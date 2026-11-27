# Fixture: typescript_body_local_class_shadow

Spec: RIPR-SPEC-0095

Owner: analysis-fixtures

Issue: #14083

## Given

The constructor of `Url` in `src/url.ts` changes to lower-case the parsed
scheme. The one test imports `Url` from `../src/url` and then declares a
body-local `class Url` of the same name, so its `new Url(..)` builds the local
class and never runs the changed constructor.

## When

`cargo xtask fixtures typescript_body_local_class_shadow` runs the public diff analyzer.

## Then

The finding reads `no_static_path` with no related test: the import binding
`Url` is not the class the test constructs, because the test body declares a
class of that name (#14083 A1). The control
`typescript_body_local_class_shadow_control` is the same test without the local
class and keeps a direct owner call.

```expected-classes
probe:src_url.ts:typescript_preview:c3882ec3 -> no_static_path
```

## Must Not

- Relate the test to the owner, or read `exposed` or `weakly_exposed`, when the
  test constructs a body-local class that shadows the imported one.
- Claim a runtime mutation outcome; this is static evidence only.
