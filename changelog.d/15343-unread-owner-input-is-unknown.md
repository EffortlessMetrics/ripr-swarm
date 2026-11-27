<!-- section: Fixed -->
- A boundary that a test may pass to the changed function through an input ripr does not read (an rstest `#[fixture]` parameter, a same-file wrapper macro such as `check!(100, 0)`, or a macro nested in an assertion) now reads as unknown instead of a false "no boundary input" gap (#15343, #15344).
