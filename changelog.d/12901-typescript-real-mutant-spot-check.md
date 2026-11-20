<!-- section: Internal / CI -->
- Mutation spot check: `cargo xtask mutation-spot-check --stryker-report <name>=<report>` scores ripr's TypeScript verdicts against real Stryker outcomes through a synthetic-diff verdict join, kept apart from the Rust span-join figures; a weekly TypeScript workflow and pinned corpus are added but have not run yet (#12901).
