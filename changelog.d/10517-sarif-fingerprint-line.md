<!-- section: Changed -->
- SARIF/GitHub fingerprints (`riprFingerprintV1`) no longer include the line number, so a pure line move no longer mints a new code-scanning alert or orphans the existing suppression; identity stays with the line-stable `finding_id`/`seam_id` (#10517).
