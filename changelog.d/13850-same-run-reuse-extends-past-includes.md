<!-- section: Changed -->
- Diff checks: same-run reuse of the admission core now extends into the main index past `include!` directives that leave every retained file's include view unchanged, instead of rebuilding it, and the include-directive and embed-file scans are memoized within a run (#12947, #13850).
