<!-- section: Changed -->
- An attempt that follows a new-test proposal now finishes only with a caller-reported `--test-result` that counted a test and failed none. Attempts that follow an existing test target keep the result optional, and `ripr continue` and the printed `--phase after` commands still do not pass the flag (#13203).
