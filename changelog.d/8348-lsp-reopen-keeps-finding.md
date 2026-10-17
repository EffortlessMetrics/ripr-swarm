<!-- section: Fixed -->
- Reopening an unchanged Rust file, or opening a second changed file the last refresh already analyzed, no longer replaces its finding with a "paused: save a new file first" marker. A clean open now adopts the analyzed identity the committed snapshot already holds for exactly those saved bytes (#8348).
