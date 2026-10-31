<!-- section: Changed -->
- Cold `ripr pilot` no longer re-copies and re-scans a whole test file for every inherent-receiver probe to decide whether the owner's `impl` is a root item of that file; the root `impl` line ranges are built once per file snapshot. Findings and artifacts are unchanged (#5361).
