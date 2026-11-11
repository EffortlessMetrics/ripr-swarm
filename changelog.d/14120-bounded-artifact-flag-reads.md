<!-- section: Fixed -->
- Twelve artifact flags (`receipt check --path`/`--ledger`, `annotations --comments`
  and `--check --out`, `plus --gap-ledger`/`--repo-exposure-summary`,
  `feedback export --route-quality`, `pr-comments existing --raw`,
  `gate evaluate` inputs, `pr-summary --baseline`,
  `impacted-evidence --pr-evidence`, `first-pr --check-output`/`--gap-ledger`)
  now read through the shared 256 MiB input limit. Pointed at an endless device
  or a multi-gigabyte file they used to grow until the system ended the
  process; they now fail with exit 2 and name the limit, or report the artifact
  as missing where that was already their behavior (#14120).
