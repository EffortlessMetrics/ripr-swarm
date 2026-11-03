<!-- section: Fixed -->
- Variant-field pattern credit no longer treats a foreign glob `use` as cleared by a renamed own-package import, and the foreign-glob defeat applies only to bare callee names—not `crate`/`self`/`super`-qualified owner calls (#8053, RIPR-SPEC-0225 decision 8).
