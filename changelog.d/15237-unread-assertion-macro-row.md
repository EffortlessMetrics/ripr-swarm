<!-- section: Fixed -->
- Rust related-test rows: a test whose only check is an assertion-like macro ripr does not read, such as a local `assert_delta!` built from `if .. { panic!() }`, no longer reads "has no assertion". The row now says that ripr could not establish that the assertion counts, and the finding names the macro. No credit or class changes (#15237).
