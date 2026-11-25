<!-- section: Fixed -->
- On `weakly_exposed` and `reachable_unrevealed` findings, a related test linked only by file, module or name now reads "no established call path to the changed code" instead of an assertion-level reason such as a missing exact assertion, which suggested it was meant to check the changed value (#16213).
