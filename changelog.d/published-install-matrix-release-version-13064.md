<!-- section: Internal / CI -->
- `published-install-matrix.yml` takes an optional `release_version` input. After a publication it pins the cargo, PyPI and npm cells to that exact release and fails any cell whose `ripr --version` differs, so one dispatch proves the registries serve the announced version; `docs/INSTALLATION_VERIFICATION.md` documents the post-publish step (#13064).
