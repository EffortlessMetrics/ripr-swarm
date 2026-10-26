# Fixture: python_table_reflection_multiline_import

Spec: RIPR-SPEC-0028

## Given

A table module re-exported by a bridge whose parenthesized import names the module only on a continuation line. A second module reaches the table through the bridge with `getattr(hub, "RA" + "TES")` and stores the original price.

## When

The silver entry changes from 5 to 6.

## Then

The finding reads `static_unknown`. Import links are read from whole statements, so the bridge connects to the table module and the reflective store is in its component.

## Must Not

Read import links from the first physical line only, or promote the edit to `exposed`.

These are static analyzer stimuli, not executed pytest fixtures. RIPR-SPEC-0108 pins the finding independently of golden blessing. Native pytest on the unchanged test passes on the original and the edited module.
