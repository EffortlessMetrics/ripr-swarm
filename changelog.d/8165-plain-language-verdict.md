<!-- section: Changed -->
- The default `ripr check` human output now uses plain language: the summary, triage state, finding digest, hidden list, and analysis-outcome line no longer leak snake_case schema tokens (such as `weakly_exposed` or `top_gap`). `--format human-full`, JSON, and `ripr explain` keep the full schema tokens unchanged (#8165).
