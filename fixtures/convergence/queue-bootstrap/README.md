# Historical label-route workflow fixtures

These files are replay inputs outside `.github/workflows`; they have no Actions
subscriptions. Tests that read them establish the former bootstrap contract,
not current queue qualification or installed protection.

The caller starts from preparation commit
`2c11644f6bf240d5d4b3a4a9f71f09e5a472845f`, with the terminal HTTP 401
inventory repair from landed commit
`64593859a1eb278837ef308ac9d127ab1ccc0a53` (#13477). This preserves the
landed repair's executable regression after the live router is retired; it
does not restore a runner-inventory request to queue qualification.
The conflict notice and watchdog
are copied from preparation commit
`64a196bb3bdb06f2d7fd8672d286c5dab5a60031`, including the upstream withdrawal
repair. Their production workflows remain retired in the cutover.

Current queue event/portfolio policy reads the live caller through repo-policy.
The live queue-resolution and terminal-result replays bind native-G composition,
PR identity, mandatory results and current eligibility separately. No passing
historical replay substitutes for those controls or hosted qualification.
