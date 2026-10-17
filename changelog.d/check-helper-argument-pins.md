<!-- section: Changed -->
- A test-local check helper that compares two of its own parameters now pins
  the owner at the test's call: `assert_json_eq(actual, expected)` with
  `let actual = format!("{{\"score\":{}}}", score(2));` and
  `let expected = String::from("{\"score\":4}");` reads as an exact pin on
  `score(2)`, and a `format!` template with fixed text around one `{}` now
  pins a scalar owner directly too (RIPR-SPEC-0197, RIPR-SPEC-0231 example 12).
