<!-- section: Fixed -->
- Verdicts: ripr credits more `assert_eq!` oracles that real tests run and
  says why when it does not. A `macro_rules! assert_eq` confined to one
  inline test module (regex-syntax), a private `use` of another `assert_eq`
  inside one test module (rust-hex), a glob import from an indexed sibling
  workspace crate, and a Rust 2021 file that uses `gen` as an identifier no
  longer make every `assert_eq!` in the workspace unestablished. A macro whose
  arguments mention `assert_eq!` (ripgrep's `rgtest!`) still does, now named
  in the refusal: its expansion can define a different `assert_eq!` from
  those tokens. `#[macro_use]` or `#[no_implicit_prelude]` inside a macro's
  arguments now refuses every trusted assertion, as an unresolved
  `#[macro_use]` does. `assert_eq!` inside a `loop` first iteration, and
  `#[macro_use]` on a resolved module, are admitted with compiled runtime
  controls. `pretty_assertions::assert_eq` imported under its own name counts
  as the standard assertion when the crate's `Cargo.toml` declares
  `pretty_assertions` as a plain registry dependency; a renamed package,
  `path`, `git`, `registry` or `[patch]` source still refuses, and says so.
  An exported (`#[macro_export]`) redefinition still refuses every
  `assert_eq!` in the crate (#5353, #5359).
- Verdicts: a refused `assert_eq!` is named with its blocker, for example
  "the test carries `#[cfg(feature = "std")]`" or "`#[macro_use] extern crate
  other;` at src/other.rs:3", in a `Not credited:` line and an `assertion not
  credited:` evidence entry. A refused context no longer reports "no detected
  assertion observes the changed value" or "no relevant oracle was detected"
  (#5359).
- Char and byte literals (`b'9'`) are literal facts again, so a predicate
  such as `digit > b'9'` has a visible boundary. Comment and string masking had
  hidden them since 0.10 (#5359).
- The all-no-path note counts statically linked related tests before bounded
  packing and names the matched assertion rows separately, so it no longer
  says "8 related tests" beside a finding that lists 81 rows (#5359).
