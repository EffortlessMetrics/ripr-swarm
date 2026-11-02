<!-- section: Fixed -->
- Instant and other partial-index diff runs abstain as `static_unknown` instead of reporting a definite `no_static_path` when unchanged test files were excluded from the index (#5316).
- Verdict-corpus row `spec0172-ex04-nested-package-owner-not-admitted` now expects `static_unknown` with `changed_since_labeling: true` under the same partial-index honesty rule (#5316).
