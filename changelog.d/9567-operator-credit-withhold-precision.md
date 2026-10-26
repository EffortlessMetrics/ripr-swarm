<!-- section: Fixed -->
- Withholding an unconfirmed operator credit now applies only when infection is read and propagation is read or weak (the discriminator is the unconfirmed leg), and finds the related test by name, file and line, so a changed line behind a branch the test never takes keeps its `weakly_exposed` gap and same-named tests in one file are not confused (RIPR-SPEC-0240, #9567).
