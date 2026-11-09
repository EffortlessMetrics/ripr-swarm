<!-- section: Fixed -->

- A test that reaches an inherent method through a same-file helper that builds its own receiver (`let re = Regex::new(..); re.is_match_with(..)`) is now related as a direct owner call when the test file imports the owner's own type; it no longer reads as linked by name only (#13411, 3/3).
