<!-- section: Fixed -->
- The TypeScript shadow check (`local_identifier_declared_in_test_body`) no longer lexes and literal-masks a test body for a candidate name the body never spells; on the zod `--base HEAD~1 --mode draft` repro the warm run measured about 1.02 s to 0.91 s on one host (`ripr check` JSON byte-identical), a partial step on #10925.
