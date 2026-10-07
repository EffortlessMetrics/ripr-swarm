<!-- section: Fixed -->
- `ripr check --json` now emits a parseable refusal document on stdout for
  every post-argv-parse failure instead of exit 2 with empty stdout. The
  envelope reuses the scope-guard shape with a typed failure identity
  (`base_unresolvable`, `repository_root_unusable`, `config_invalid`,
  `suppression_policy_invalid`, `git_invocation_timeout`, or the
  `analysis_failed` fallback), a repair route, and `downstream_consumable:
  false`; the human diagnostic on stderr is unchanged. Argv usage errors
  stay prose-only (#6834).
