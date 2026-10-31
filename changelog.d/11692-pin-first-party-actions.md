<!-- section: Changed -->
- CI supply chain: every external `uses:` in `.github/workflows` and `.github/actions`, first-party `actions/*` included, is pinned to a full commit SHA with a version comment, and `check-workflows` now rejects an unpinned `actions/*` reference (#11692).
