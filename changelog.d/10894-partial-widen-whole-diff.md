<!-- section: Changed -->
- A partial-scope result now names, beside the smallest budgets that admit the next file, the budget that analyzes the whole diff whenever the analysis-cost limits allow it, so one re-run completes. On a 20-commit Django diff the hint read "at least 1015", which stayed partial; it now reads "at least 1015 (1041 for the whole diff)", and 1041 completes (#10894).
