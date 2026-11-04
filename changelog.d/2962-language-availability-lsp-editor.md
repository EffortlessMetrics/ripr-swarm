<!-- section: Changed -->
- LSP `ripr/analysisStatus` and workspace status now project `availability_state` and ordered `remediation` from the shared `LanguageAvailabilityReport` on each `unavailable_languages` row, while keeping the editor recovery sentence for setup actions (#2962, #2615).
- VS Code setup diagnosis and language-not-enabled status prefer the server remediation steps when building the next safe action (#2962).
