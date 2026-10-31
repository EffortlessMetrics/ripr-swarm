<!-- section: Fixed -->
- The TypeScript preview refusal for an unbound boundary operand now leads with the remedy ("derive an input where `a == b` holds") before the cause, so the one-line `Safe next action` in `ripr check` no longer cuts the remedy off at its length budget. The full reason is unchanged in `--format human-full` and JSON (#11592).
