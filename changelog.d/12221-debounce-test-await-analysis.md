<!-- section: Internal / CI -->
- The LSP interactive-debounce fold test now cancels the refresh scheduler and waits behind the single analysis thread before it returns, so it no longer exits with analysis work in flight. On Windows nextest reported that test LEAK in 42–51% of runs; a 100-iteration stress run with the wait shows 0 LEAK rows (#12221).
