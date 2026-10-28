# Fixture: python_table_reexport_reflection

Spec: RIPR-SPEC-0028

## Given

A table module re-exported through `rates/bridge.py`. A second module imported by the test reaches the table through the re-exported module with `getattr(hub, "RA" + "TES")` and stores the original price.

## When

The silver entry changes from 5 to 6.

## Then

The finding reads `static_unknown`. The owner module is named only in the bridge and the reflective spelling only in the store, so no single file names both. A file that is reflective and imports modules may reach any table through an alias, and the credit is withheld.

## Must Not

Credit a table when a reflective file imports modules the guard cannot trace to the owner, or promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures; the verdict corpus (`authored-py-table-traps`) carries the runtime truth. RIPR-SPEC-0108 pins the finding independently of golden blessing.
