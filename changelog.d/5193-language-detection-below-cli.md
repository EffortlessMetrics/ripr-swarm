<!-- section: Changed -->
- Shallow language detection (`detect_languages` and the Perl project probe) moved from the `doctor` CLI command into `config::language_detection`, so `first-pr` output no longer imports the CLI, and `check-architecture` now rejects `output` → `cli` imports. Detection results are unchanged (#5193).
