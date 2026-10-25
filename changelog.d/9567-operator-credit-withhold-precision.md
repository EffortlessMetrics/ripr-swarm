<!-- section: Fixed -->
- Withholding an unconfirmed operator credit now keys on the finding's own observation state and matches the related test by name, file and line, so a test that never drives the changed branch keeps its `weakly_exposed` gap and same-named tests in one file are not confused (RIPR-SPEC-0240, #9567).
