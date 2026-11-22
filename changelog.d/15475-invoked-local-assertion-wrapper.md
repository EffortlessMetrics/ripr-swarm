<!-- section: Fixed -->
- Rust related-test rows: a test that defines a local `macro_rules!` wrapper around an assertion-like macro ripr does not read, then invokes the wrapper, no longer reads "has no assertion". The row reads like a direct call of that macro and names the wrapped macro. No credit or class changes (#15475).
