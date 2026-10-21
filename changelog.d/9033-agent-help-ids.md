<!-- section: Changed -->
- `ripr agent --help` and the `ripr agent repair --phase before` missing-`--seam-id` refusal no longer say `ripr check` finding IDs are not seam IDs, which contradicted `--phase before` resolving them; the group help now lists `ripr repair` then `ripr continue` as the repair path to start with, above the explicit-phase spelling (#9033).
