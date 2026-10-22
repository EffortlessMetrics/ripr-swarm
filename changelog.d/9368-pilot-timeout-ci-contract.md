<!-- section: Fixed -->
- A `ripr pilot` run that hits its timeout now prints the exit-code CI contract on stderr when it happens: the partial packet is written and the run exits 0 deliberately, so CI must gate on `pilot-summary.json` top-level `status` (`complete` vs `partial`), not the exit code (#9368).
