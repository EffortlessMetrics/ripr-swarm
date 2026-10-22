<!-- section: Fixed -->
- `ripr explain` and `ripr context` now answer a seam-shaped selector (16 lowercase hex digits, or `seam:` plus them) that matches no finding with a message naming `ripr repair <id>` and `ripr agent repair --seam-id <id>`, bound to the analyzed root. Only the validated id is echoed into the suggested commands; other selectors keep the `ripr check` listing hint (#8522).
