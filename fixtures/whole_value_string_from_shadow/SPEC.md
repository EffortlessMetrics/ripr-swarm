# Fixture: whole_value_string_from_shadow

Spec: RIPR-SPEC-0225

## Given

Production code changes a `String` field initializer of a returned struct
literal (`name: std::string::String::new()` to `name: render(retries)`)
in `build`:

```rust
pub fn build(retries: u32) -> Config {
    Config {
        retries,
        name: render(retries),
    }
}
```

The related test compares the owner's whole result with an expected
literal of the same type whose `name` value is `String::from("3")`. The
test module declares `mod String`, so `String::from` resolves to a
test-local function that returns `build`'s own rendered name, not to the
standard library:

```rust
mod String {
    pub fn from(value: &str) -> std::string::String {
        super::build(value.trim().parse().unwrap_or(0)).name
    }
}

#[test]
fn builds_config() {
    assert_eq!(
        build(3),
        Config { retries: 3, name: String::from("3") }
    );
}
```

## When

```bash
ripr check --root fixtures/whole_value_string_from_shadow/input \
           --diff fixtures/whole_value_string_from_shadow/diff.patch --mode fast
```

## Then

`ripr` must NOT read the changed `name:` initializer as `exposed` through
the whole-value field pin. Both `assert_eq!` operands obtain the field
from `build`, so the test cannot fail for any mutant of the changed line
and the expected value is not independent. `String::from(..)` counts as a
string-literal conversion only when `String` names the standard type; a
workspace `mod String` refuses the pin and the finding stays
`weakly_exposed`.

This fixture is the **should-stay-`weakly_exposed` control** for the
`String::from` branch of RIPR-SPEC-0225's independent-value rule: entity
identity cannot be inferred from the spelling `String::from`.

## Must Not

- Promote the `name:` field finding to `exposed` through the whole-value
  literal comparison.
- Treat `String::from(..)` as the standard conversion when a workspace
  module, type or rename may own the name.
- Use mutation-runtime outcome vocabulary.
