# check-fast report

Status: pass

Selector: passed
Base: origin/main

Ran:
- fmt --check
- check-static-language
- check-command-catalog
- check-no-panic-family
- check-allow-attributes
- check-file-policy
- clippy
- check-workflows
- check-covered-by
- check-process-policy
- check-network-policy
- check-generated
- check-generated-clean
- check-lint-policy

Skipped:
- check-fixture-contracts
