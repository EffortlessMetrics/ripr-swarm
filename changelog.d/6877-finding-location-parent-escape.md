<!-- section: Fixed -->
- The shared finding-location owner renders a file whose root-relative
  remainder contains `..` in its full stable spelling instead of serving
  a workspace-joined `./..` escape on the wire: such a remainder
  resolves outside the analyzed root, so joining it against the
  workspace would name a file outside the workspace (#6877).
