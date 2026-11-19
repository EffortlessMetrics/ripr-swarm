<!-- section: Internal / CI -->
- The TypeScript spot-check workflow installs with `--legacy-peer-deps` and pins Stryker 9.6.1, because the first hosted run hit a peer-dependency conflict and Stryker 8.7.1's vitest runner crashed on the corpus's vitest (#12901).
