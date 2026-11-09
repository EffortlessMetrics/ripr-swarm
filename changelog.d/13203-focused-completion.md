<!-- section: Fixed -->
- Refuse incomplete or contradictory caller-reported focused test event sequences (a test reporting both `ok` and `failed`, a `started` test with no terminal event, or an unclosed or mismatched suite) before finishing a repair attempt. `--test-result` remains optional and does not gate finish (#13203).
