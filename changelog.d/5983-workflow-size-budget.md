<!-- section: Changed -->
- Split the generated workflow's line budget out of `first_run.over_budget_steps` into its own `first_run.over_budget_workflows` metric, so a large `init_ci` file no longer reads as a slow or noisy first-run step. `over_budget_steps` now counts only time and output-length budgets. The committed first-run baseline is updated to match (#5983).
