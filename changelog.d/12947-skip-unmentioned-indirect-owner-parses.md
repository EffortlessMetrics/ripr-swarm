<!-- section: Changed -->
- Rust checks: the withheld indirect-owner pass skips parsing a test body that never mentions the owner's name, so a large diff no longer re-parses every test for each withheld probe; findings are unchanged (#12947).
