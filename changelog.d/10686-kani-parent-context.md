<!-- section: Fixed -->
- Declared Kani source inventories with external or unresolved module ancestry
  no longer infer enabled applicability from child-file attributes alone.
  They retain `native_parent_context_unmodeled`; file-local false configuration
  can still establish disabled applicability. Cross-file native configuration
  evaluation remains unsupported by this profile (#10686).
