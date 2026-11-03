# Packaged repair channel receipt fixture

Spec: RIPR-SPEC-0207

Issues: #4648, #4650

## Given

The cross-channel packaged repair qualification slice needs one versioned
receipt schema and manifest so installed-CLI journeys can emit comparable
receipts before VSIX parity work lands.

## When

`cargo xtask check-fixture-contracts` validates `manifest.json` and
`channel-receipt.schema.json`. The `packaged_repair_cli` integration tests emit
`target/ripr/reports/packaged-repair-channel-receipt.json` on the positive
journey with `vsix_parity_claimed: false`.

## Then

The dedicated validator accepts the manifest schema version and pins
`vsix_parity_claimed` to false in the JSON schema.

## Must Not

This fixture must not claim VSIX/editor-host qualification, dogfood promotion,
or release readiness by itself.
