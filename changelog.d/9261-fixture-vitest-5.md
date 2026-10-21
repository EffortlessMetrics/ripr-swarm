<!-- section: Fixed -->
- Fixture test inputs (`blind_journey_installed_typescript` snapshots and the
  `typescript_caught_assertion`, `typescript_finally_completion` and
  `typescript_rejects_literal_equality` inputs) move to vitest 5.0.5, the first
  line that clears the tinypool and `@vitest/mocker` advisories. Snapshot
  digests, reproducible git commit/tree identities, the recorded runner
  identity and the executor corpus bindings were re-derived together (#9261).
