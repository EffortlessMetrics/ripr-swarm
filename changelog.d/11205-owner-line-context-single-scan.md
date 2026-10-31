<!-- section: Changed -->
- `ripr check` on TypeScript/JavaScript workspaces now scans the related-test candidates once per owner instead of twice: the related mock paths are projected from the candidates the line context already holds, which cuts about a third of the instructions on a warm zod run with unchanged output (#11205, #10925).
