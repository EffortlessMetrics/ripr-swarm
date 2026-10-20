<!-- section: Internal / CI -->
- A unit test pins that a test carrying `#[cfg(feature = "serde")]` (rand `uniform_other.rs` `test_char_bad_deser`, #8388) is refused for its attribute, an analyzer limit rather than a macro rebinding, while the same test without the gate is credited and a foreign glob still refuses.
