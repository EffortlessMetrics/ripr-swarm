# Fixture: typescript_config_relative_paths

Spec: RIPR-SPEC-0099

## Given

The selected root config maps `@/*` to `./src/*` without `baseUrl`.
The changed `price` returns 0.2, while the aliased test
independently requires the previous literal 0.1. Resolving that explicit
config target identifies the production owner; the test's exact value
assertion discriminates the changed return without deriving its expectation
from production. This intent is recorded before generating the goldens.

## When

The production CLI analyzes `diff.patch` against `input`.

## Then

Exactly one `return_value` finding is `exposed`, with the aliased test related and
its strong exact-value oracle retained. TypeScript remains Preview.

## Must Not

- Treat non-relative `src/*` as valid without `baseUrl`.
- Credit a same-named decoy, ambiguous target, or unsupported winning key.
- Exempt an unselected root config from extraction-gap disclosure.

The focused resolver, adapter and real CLI tests independently exercise those
negative controls and selected-config completeness.
