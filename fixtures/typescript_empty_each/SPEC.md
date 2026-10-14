# Fixture: typescript_empty_each

Spec: RIPR-SPEC-0027

## Given

The changed predicate includes age 18. All apparent exact boundary assertions
are inside inline empty `.each` tables, including focused tests, parameterized
suites, and erased TypeScript wrappers. Vitest 4.1.11 registers cases inside a
`cases.forEach` loop: these tables produce zero callbacks. An independent native
runner observation and 15-case public CLI oracle are retained with issue #7596.

## When

`cargo xtask fixtures typescript_empty_each` runs the production CLI on the diff.

## Then

Exactly one predicate finding remains `no_static_path`, with reach `no`, no
related tests, and oracle `unknown` / `none`. The empty suites do not create a
partial-extraction warning. Literal honesty-corpus assertions pin these values
independently of the golden files. Rust adapter tests separately retain active
`[[]]`, scalar tables, ordinary suites and independent ordinary siblings.

## Must Not

- Credit callback assertions that have zero registered cases.
- Confuse zero rows `[]` with one empty row `[[]]`.
- Claim evaluation of named/dynamic tables or runtime selection generally.
- Change TypeScript preview support or infer runtime mutation adequacy.
