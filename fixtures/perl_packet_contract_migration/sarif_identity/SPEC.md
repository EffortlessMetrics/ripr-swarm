# SARIF fingerprint identity controls

These contract fixtures derive the packet vocabulary from the retained
`perl_packet_contract_migration` producer snapshot. They are deliberately
one-based, digest-verified packet/source/diff pairs, not a live producer run.
Test/oracle arrays are empty so each change takes the same no-test path.

`baseline_one` contains one changed behavior, `current_two` contains two
different return changes in the same named owner, and the two shifted cases
move those changes before or within that owner. File content digests and
structural packet fingerprints are checked by normal ingestion; currentness
must match actual source and parsed added lines before SARIF is consumed.

The policy test requires two distinct keys, exactly one new result versus the
one-change baseline, stable keys across supported shifts and exact native
suppression selectors. This does not qualify the Perl exporter, prove opaque
producer digest correctness or promise collision-free identities.
