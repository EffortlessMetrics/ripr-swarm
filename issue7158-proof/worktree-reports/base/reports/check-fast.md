# check-fast report

Status: pass

Selector: passed
Base: origin/main

Ran:
- fmt --check
- check-static-language
- check-command-catalog
- check-generated
- check-generated-clean
- check-lint-policy

Skipped:
- check-no-panic-family
- check-allow-attributes
- check-file-policy
- clippy
- check-fixture-contracts
