# Fixture: python_table_bool_key_get_fallback

Spec: RIPR-SPEC-0028

## Given

A module dict constant read through `.get(tier, BOOL_TABLE[2])`. The test calls the reader with `True`, which Python hashes as integer key 1.

## When

The fallback entry for key 2 changes from 5 to 6.

## Then

The finding reads `static_unknown`. `True` selects key 1, so the changed fallback is never read and the assertion holds before and after. The analyzer must treat a key type it cannot evaluate as unknown, not as a missing key.

## Must Not

Read the unsupported bool key as absent and take the changed fallback, or promote the edit to `exposed`. The runtime pair (original and edited table) both pass this test.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native pytest on the unchanged test passes on the original and the edited module, so this is a trap and not a kill.
